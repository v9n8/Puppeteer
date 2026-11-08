//! Export a run's conversation to a Markdown file the user can keep or share.
//!
//! The file holds what a reader sees of the conversation: the user's prompts
//! and the assistant's replies, read from the persisted log. Tool input /
//! output and thinking are left out — they are where file bodies, `.env`
//! contents and command lines with credentials end up. The secret notes vault
//! never reaches a log, so it cannot reach an export either. What is left still
//! goes through [`redact`]: a key pasted into a prompt, or echoed by the model,
//! must not travel with the file.
//!
//! The destination is chosen here, never by the caller (a Controller may run
//! the export): `<project>/.puppeteer/exports/`, next to the run logs.

use std::io::BufRead;
use std::path::Path;

use serde_json::Value;
use tauri::State;

use crate::db::Db;

const REDACTED: &str = "[redacted]";

/// Credential prefixes recognised in text.
const TOKEN_PREFIXES: &[&str] = &[
    "sk-", "sk_live_", "sk_test_", "rk_live_", // OpenAI / Anthropic, Stripe
    "ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_", // GitHub
    "glpat-", // GitLab
    "xoxb-", "xoxp-", "xoxa-", "xoxs-", "xoxe-", "xapp-", // Slack
    "AKIA", "ASIA", // AWS access key ids
    "AIza", // Google API keys
    "npm_", "hf_", "pypi-",
];
/// Shortest token body after a prefix: shorter is a word, not a key.
const TOKEN_MIN_BODY: usize = 16;
/// `NAME=value` assignments redacted when NAME (upper-cased) contains one of these.
const SECRET_NAMES: &[&str] =
    &["TOKEN", "SECRET", "PASSWORD", "PASSWD", "API_KEY", "APIKEY", "PRIVATE_KEY", "ACCESS_KEY", "CREDENTIAL"];
const MAX_NAME: usize = 64;
const MAX_VALUE: usize = 4096;
const MAX_AUTHORITY: usize = 256;

fn is_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn run_of(s: &[u8], from: usize, max: usize, ok: impl Fn(u8) -> bool) -> usize {
    let mut end = from;
    while end < s.len() && end - from < max && ok(s[end]) {
        end += 1;
    }
    end
}

fn find(s: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    s.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|p| from + p)
}

/// A PEM private key block starting at `i`, to its END line (or the text's end).
fn pem_block(s: &[u8], i: usize) -> Option<usize> {
    if !s[i..].starts_with(b"-----BEGIN ") {
        return None;
    }
    let header_end = find(s, i + 11, b"-----").filter(|e| e - i < 80)?;
    if find(&s[..header_end], i, b"PRIVATE KEY").is_none() {
        return None;
    }
    Some(match find(s, header_end + 5, b"-----END ") {
        Some(end) => find(s, end + 9, b"-----").map_or(s.len(), |e| e + 5),
        None => s.len(),
    })
}

fn prefixed_token(s: &[u8], i: usize) -> Option<usize> {
    TOKEN_PREFIXES.iter().find_map(|p| {
        let body = i + p.len();
        if !s[i..].starts_with(p.as_bytes()) {
            return None;
        }
        let end = run_of(s, body, MAX_VALUE, is_token_byte);
        (end - body >= TOKEN_MIN_BODY && s[body..end].iter().any(u8::is_ascii_digit)).then_some(end)
    })
}

/// `eyJ…`.`…`.`…`: a JSON Web Token.
fn jwt(s: &[u8], i: usize) -> Option<usize> {
    if !s[i..].starts_with(b"eyJ") {
        return None;
    }
    let mut end = i;
    for part in 0..3 {
        if part > 0 {
            if s.get(end) != Some(&b'.') {
                return None;
            }
            end += 1;
        }
        let next = run_of(s, end, MAX_VALUE, is_token_byte);
        if next - end < 8 {
            return None;
        }
        end = next;
    }
    Some(end)
}

/// `Bearer <token>`: the word kept, the token redacted.
fn bearer(s: &[u8], i: usize) -> Option<(usize, String)> {
    let word = s.get(i..i + 7)?;
    if !word.eq_ignore_ascii_case(b"bearer ") {
        return None;
    }
    let end = run_of(s, i + 7, MAX_VALUE, |b| is_token_byte(b) || b"._~+/=".contains(&b));
    (end - (i + 7) >= TOKEN_MIN_BODY).then(|| (end, format!("{}{REDACTED}", String::from_utf8_lossy(word))))
}

/// `NAME=value` where NAME names a secret, e.g. `DB_PASSWORD=hunter2!`. A value
/// that is a variable (`$TOKEN`), a plain number (`max_tokens=4096`) or only
/// letters (a name, not a secret) is left alone.
fn assignment(s: &[u8], i: usize) -> Option<(usize, String)> {
    let name_end = run_of(s, i, MAX_NAME + 1, |b| b.is_ascii_alphanumeric() || b == b'_');
    if name_end == i || name_end - i > MAX_NAME || s.get(name_end) != Some(&b'=') {
        return None;
    }
    let name = String::from_utf8_lossy(&s[i..name_end]).to_ascii_uppercase();
    if !SECRET_NAMES.iter().any(|n| name.contains(n)) {
        return None;
    }
    let mut start = name_end + 1;
    let quote = s.get(start).copied().filter(|b| *b == b'"' || *b == b'\'');
    if quote.is_some() {
        start += 1;
    }
    let end = run_of(s, start, MAX_VALUE, |b| !b.is_ascii_whitespace() && b != b'"' && b != b'\'');
    let value = &s[start..end];
    if value.is_empty()
        || value[0] == b'$'
        || value.iter().all(u8::is_ascii_digit)
        || value.iter().all(u8::is_ascii_alphabetic)
    {
        return None;
    }
    Some((end, format!("{}{REDACTED}", String::from_utf8_lossy(&s[i..start]))))
}

/// `scheme://user:password@host`: the user info redacted.
fn url_credentials(s: &[u8], i: usize) -> Option<(usize, String)> {
    if !s[i..].starts_with(b"://") {
        return None;
    }
    let from = i + 3;
    let end = run_of(s, from, MAX_AUTHORITY, |b| !b.is_ascii_whitespace() && !b"/\"'<>".contains(&b));
    let at = s[from..end].iter().rposition(|b| *b == b'@')? + from;
    s[from..at].contains(&b':').then(|| (at, format!("://{REDACTED}")))
}

/// `text` with the credentials it shows replaced by `[redacted]`: known key
/// prefixes, JWTs, bearer tokens, PEM private keys, passwords in URLs and
/// secret-named `NAME=value` assignments. Bare high-entropy strings are kept:
/// they are mostly hashes and ids, which a reader of the transcript needs.
pub fn redact(text: &str) -> String {
    let s = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let hit = url_credentials(s, i).or_else(|| {
            if i > 0 && is_token_byte(s[i - 1]) {
                return None;
            }
            pem_block(s, i)
                .map(|end| (end, "[redacted private key]".to_string()))
                .or_else(|| prefixed_token(s, i).or_else(|| jwt(s, i)).map(|end| (end, REDACTED.to_string())))
                .or_else(|| bearer(s, i))
                .or_else(|| assignment(s, i))
        });
        match hit {
            Some((end, replacement)) => {
                out.extend_from_slice(replacement.as_bytes());
                i = end;
            }
            None => {
                out.push(s[i]);
                i += 1;
            }
        }
    }
    // Every cut falls on an ASCII byte, so the bytes are still UTF-8.
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// Post-compaction summary the engine injects as a user message.
const COMPACT_PRELUDE: &str = "This session is being continued from a previous conversation";

enum Piece {
    Prompt(String),
    Reply(String),
}

fn texts(content: Option<&Value>) -> (Vec<&str>, usize, bool) {
    let (mut texts, mut images, mut tool) = (Vec::new(), 0, false);
    match content {
        Some(Value::String(s)) => texts.push(s.as_str()),
        Some(Value::Array(blocks)) => {
            for b in blocks {
                match b.get("type").and_then(Value::as_str) {
                    Some("text") => texts.extend(b.get("text").and_then(Value::as_str)),
                    Some("image") => images += 1,
                    Some("tool_result") => tool = true,
                    _ => {}
                }
            }
        }
        _ => {}
    }
    (texts.into_iter().map(str::trim).filter(|t| !t.is_empty()).collect(), images, tool)
}

/// What one log record adds to the transcript: a prompt the user sent, or
/// reply text of the main conversation (a subagent's is part of its tool call).
fn piece(v: &Value) -> Option<Piece> {
    if v.get("parent_tool_use_id").is_some_and(|p| !p.is_null()) {
        return None;
    }
    let flag = |k: &str| v.get(k).and_then(Value::as_bool) == Some(true);
    match v.get("type").and_then(Value::as_str)? {
        "user" => {
            if flag("isCompactSummary") || flag("isMeta") || flag("isSynthetic") || v.get("note").is_some() {
                return None;
            }
            let (parts, images, tool) = texts(v.pointer("/message/content"));
            let text = parts.join("\n\n");
            if tool
                || text.starts_with(COMPACT_PRELUDE)
                || text.contains("<command-name>")
                || text.contains("<local-command-")
            {
                return None;
            }
            let mut lines: Vec<String> = Vec::new();
            if !text.is_empty() {
                lines.push(text);
            }
            lines.extend(std::iter::repeat_n("_[image]_".to_string(), images));
            (!lines.is_empty()).then(|| Piece::Prompt(lines.join("\n\n")))
        }
        "assistant" => {
            let (parts, _, _) = texts(v.pointer("/message/content"));
            (!parts.is_empty()).then(|| Piece::Reply(parts.join("\n\n")))
        }
        _ => None,
    }
}

pub struct Header<'a> {
    pub title: &'a str,
    pub project: &'a str,
    pub engine: &'a str,
    pub run_id: &'a str,
    pub exported_at: &'a str,
}

/// The transcript of `log` (JSON records, one per line) as Markdown: a header,
/// then one `## Turn n` section per prompt with the prompt under **You** and
/// the replies to it under **Assistant**. Everything is redacted.
pub fn to_markdown(log: impl BufRead, header: &Header) -> std::io::Result<String> {
    let mut out = format!(
        "# {}\n\n- Project: {}\n- Engine: {}\n- Session: {}\n- Exported: {}\n",
        redact(header.title),
        header.project,
        header.engine,
        header.run_id,
        header.exported_at
    );
    let mut turn = 0;
    let mut replied = false;
    for line in log.split(b'\n') {
        let line = line?;
        if line.first() != Some(&b'{') {
            continue;
        }
        let Some(piece) = serde_json::from_slice::<Value>(&line).ok().as_ref().and_then(piece) else {
            continue;
        };
        match piece {
            Piece::Prompt(text) => {
                turn += 1;
                replied = false;
                out.push_str(&format!("\n## Turn {turn}\n\n**You**\n\n{}\n", redact(&text)));
            }
            Piece::Reply(text) => {
                if turn == 0 {
                    // Replies before any prompt (a session continued from elsewhere).
                    turn = 1;
                    out.push_str("\n## Turn 1\n");
                }
                if !replied {
                    out.push_str("\n**Assistant**\n");
                    replied = true;
                }
                out.push_str(&format!("\n{}\n", redact(&text)));
            }
        }
    }
    if turn == 0 {
        out.push_str("\n_No messages._\n");
    }
    Ok(out)
}

fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(data)
}

/// Write the run's transcript under `<project>/.puppeteer/exports/` and return
/// the file's path.
#[tauri::command]
pub async fn export_run_transcript(db: State<'_, Db>, run_id: String) -> Result<String, String> {
    let (title, engine, project_name, project_path): (Option<String>, String, String, String) = sqlx::query_as(
        "SELECT r.title, r.engine, p.name, p.path FROM runs r JOIN projects p ON p.id = r.project_id WHERE r.id = ?",
    )
    .bind(&run_id)
    .fetch_one(db.inner())
    .await
    .map_err(|e| e.to_string())?;
    let row = crate::commands::runs::fetch_run_log_row(db.inner(), &run_id).await?;
    let Some((_, log)) = crate::commands::runs::resolve_log_source(&row, &run_id) else {
        return Err("no_log".into());
    };
    let now = chrono::Utc::now();
    let dir = Path::new(&project_path).join(".puppeteer").join("exports");
    let short: String = run_id.chars().take(8).collect();
    let path = dir.join(format!("transcript-{short}-{}.md", now.format("%Y%m%d-%H%M%S")));
    let exported_at = now.to_rfc3339();
    let written = path.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let file = std::fs::File::open(&log).map_err(|e| e.to_string())?;
        let header = Header {
            title: title.as_deref().filter(|t| !t.trim().is_empty()).unwrap_or("Session"),
            project: &project_name,
            engine: &engine,
            run_id: &run_id,
            exported_at: &exported_at,
        };
        let markdown = to_markdown(std::io::BufReader::new(file), &header).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        write_private(&written, markdown.as_bytes()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    tracing::info!(event = "run_transcript_exported", path = %path.display());
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn log(records: &[Value]) -> String {
        let mut out = String::from("[stderr] starting\n");
        for r in records {
            out.push_str(&r.to_string());
            out.push('\n');
        }
        out
    }

    fn header() -> Header<'static> {
        Header { title: "Fix login", project: "web", engine: "claude", run_id: "r-1", exported_at: "2026-11-08T00:00:00+00:00" }
    }

    fn assistant(content: Value) -> Value {
        json!({ "type": "assistant", "uuid": "a", "parent_tool_use_id": null, "message": { "role": "assistant", "content": content } })
    }

    #[test]
    fn the_export_is_one_section_per_prompt_with_its_replies() {
        let text = log(&[
            json!({ "type": "system", "subtype": "init", "session_id": "s", "cwd": "/home/me/web" }),
            json!({ "type": "user", "message": { "role": "user", "content": "Why does login fail?" }, "turn": 1 }),
            assistant(json!([{ "type": "thinking", "thinking": "hmm" }, { "type": "text", "text": "Let me look." }])),
            assistant(json!([{ "type": "tool_use", "id": "t1", "name": "Bash", "input": { "command": "cat .env" } }])),
            json!({ "type": "user", "parent_tool_use_id": null, "message": { "content": [
                { "type": "tool_result", "tool_use_id": "t1", "content": "DB_PASSWORD=hunter2!" } ] } }),
            json!({ "type": "assistant", "parent_tool_use_id": "t9", "message": { "content": [{ "type": "text", "text": "subagent says" }] } }),
            assistant(json!([{ "type": "text", "text": "The cookie expired." }])),
            json!({ "type": "result", "subtype": "success", "result": "The cookie expired." }),
            json!({ "type": "user", "message": { "role": "user", "content": [
                { "type": "text", "text": "And this?" },
                { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "AAAA" } } ] } }),
            json!({ "type": "user", "isCompactSummary": true, "message": { "content": "This session is being continued from a previous conversation…" } }),
            assistant(json!([{ "type": "text", "text": "A screenshot of the form." }])),
        ]);
        let md = to_markdown(text.as_bytes(), &header()).unwrap();
        assert_eq!(
            md,
            "# Fix login\n\n- Project: web\n- Engine: claude\n- Session: r-1\n- Exported: 2026-11-08T00:00:00+00:00\n\
             \n## Turn 1\n\n**You**\n\nWhy does login fail?\n\
             \n**Assistant**\n\nLet me look.\n\nThe cookie expired.\n\
             \n## Turn 2\n\n**You**\n\nAnd this?\n\n_[image]_\n\
             \n**Assistant**\n\nA screenshot of the form.\n"
        );
    }

    #[test]
    fn an_empty_log_says_so_and_replies_before_any_prompt_get_a_turn() {
        let md = to_markdown("".as_bytes(), &header()).unwrap();
        assert!(md.ends_with("\n_No messages._\n"), "{md}");
        let md = to_markdown(log(&[assistant(json!([{ "type": "text", "text": "hi" }]))]).as_bytes(), &header()).unwrap();
        assert!(md.ends_with("\n## Turn 1\n\n**Assistant**\n\nhi\n"), "{md}");
    }

    #[test]
    fn prompts_and_replies_are_redacted() {
        let text = log(&[
            json!({ "type": "user", "message": { "content": "use ghp_abcdefghijklmnopqrstuvwxyz0123456789 please" } }),
            assistant(json!([{ "type": "text", "text": "Set OPENAI_API_KEY=sk-proj-AbC123dEf456GhI789jKl0" }])),
        ]);
        let md = to_markdown(text.as_bytes(), &header()).unwrap();
        assert!(md.contains("use [redacted] please"), "{md}");
        assert!(md.contains("Set OPENAI_API_KEY=[redacted]"), "{md}");
        assert!(!md.contains("ghp_") && !md.contains("sk-proj"), "{md}");
    }

    #[test]
    fn redact_hides_credentials_by_shape() {
        let cases = [
            ("key sk-ant-api03-AbCdEf0123456789xyz end", "key [redacted] end"),
            ("AKIAIOSFODNN7EXAMPLE1", "[redacted]"),
            ("xoxb-1234567890-abcdefghijkl", "[redacted]"),
            ("glpat-abcdefgh12345678wxyz", "[redacted]"),
            ("AIzaSyA1234567890abcdefghijklmnopqrstu", "[redacted]"),
            ("Authorization: Bearer abc.def-1234567890XYZ", "Authorization: Bearer [redacted]"),
            ("t=eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U", "t=[redacted]"),
            ("git clone https://me:s3cret@github.com/a/b", "git clone https://[redacted]@github.com/a/b"),
            ("export DB_PASSWORD=\"hunter2!\" && go", "export DB_PASSWORD=\"[redacted]\" && go"),
            (
                "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk\n-----END OPENSSH PRIVATE KEY-----\nafter",
                "[redacted private key]\nafter",
            ),
        ];
        for (input, want) in cases {
            assert_eq!(redact(input), want, "{input}");
        }
    }

    #[test]
    fn redact_keeps_what_only_looks_like_a_secret() {
        for text in [
            "commit 3f2a9c1b7d4e5f60718293a4b5c6d7e8f9012345 fixed it",
            "set max_tokens=4096 and PASSWORD=$DB_PASSWORD",
            "the token=placeholder stays, so does task-runner-version-2",
            "https://github.com/a/b and user@example.com",
            "sk-short1 and ghp_tooShort1",
            "Mật khẩu: không có ở đây — ghi chú tiếng Việt",
            "-----BEGIN PUBLIC KEY-----\nMFkw\n-----END PUBLIC KEY-----",
        ] {
            assert_eq!(redact(text), text);
        }
    }
}
