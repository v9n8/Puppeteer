//! MCP servers read from pasted setup text (UX-85), as drafts for the MCP form: a config block of
//! any tool the import reads (`mcpServers`, VS Code's `servers`, Codex's `[mcp_servers.*]`), or an
//! install command (`claude|gemini|codex mcp add …`, `claude mcp add-json …`, `code --add-mcp …`).
//! A README is fine too: its fenced blocks are tried first, then the whole text. Nothing here
//! reads a file or the network; text no format matches gives no draft, and the form asks the AI.
//!
//! Unlike an import, a draft keeps the VALUEs the text holds: the user pasted them into the
//! webview already, and the form stores them on save. A value that only says what to fill in
//! (`${TOKEN}`, `$TOKEN`, `<token>`, `YOUR_KEY`) is blanked, and the rest of it is kept as a hint
//! (`Bearer …`), so the user types the whole value — not just the token.

use super::{library_name, mcp_export, strip_jsonc};
use crate::commands::mcp::McpServerExport;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

/// Longer text is not setup instructions.
pub const MAX_TEXT_BYTES: usize = 256 * 1024;

/// One env var or header of a draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PastedValue {
    pub key: String,
    /// Empty when the text only had a placeholder.
    pub value: String,
    /// What the value looks like with the part to fill in as `…` (`Bearer …`); none when the
    /// whole value is to fill in, or nothing is.
    pub hint: Option<String>,
}

/// A server read from pasted text, in the MCP form's terms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PastedServer {
    /// Already a valid server name.
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env: Vec<PastedValue>,
    pub headers: Vec<PastedValue>,
}

/// The servers of the first block of `text` that reads as a known format; none when no block does.
pub fn parse(text: &str) -> Vec<PastedServer> {
    blocks(text)
        .iter()
        .map(|block| servers_in(block))
        .find(|servers| !servers.is_empty())
        .unwrap_or_default()
        .into_iter()
        .map(draft)
        .collect()
}

fn servers_in(block: &str) -> Vec<McpServerExport> {
    let found = from_commands(block);
    if !found.is_empty() {
        return found;
    }
    let found = from_json(block);
    if !found.is_empty() {
        return found;
    }
    from_toml(block)
}

/// The fenced code blocks of `text`, then `text` itself.
fn blocks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            match current.take() {
                Some(lines) => out.push(lines.join("\n")),
                None => current = Some(Vec::new()),
            }
        } else if let Some(lines) = current.as_mut() {
            lines.push(line);
        }
    }
    out.push(text.to_string());
    out
}

// ── Install commands ────────────────────────────────────────────────────────

/// The servers of every install command in `block`, one per line (a trailing `\` continues it).
fn from_commands(block: &str) -> Vec<McpServerExport> {
    let joined = block.replace("\\\r\n", " ").replace("\\\n", " ");
    joined.lines().filter_map(|line| words(line).and_then(|w| from_command(&w))).collect()
}

fn from_command(words: &[String]) -> Option<McpServerExport> {
    let bin = |w: &String| w.rsplit('/').next().unwrap_or(w).to_string();
    let at = words.iter().position(|w| matches!(bin(w).as_str(), "claude" | "gemini" | "codex" | "code" | "code-insiders"))?;
    let tool = bin(&words[at]);
    let rest = &words[at + 1..];
    match (tool.as_str(), rest) {
        ("code" | "code-insiders", [flag, json, ..]) if flag == "--add-mcp" => {
            let value: Value = serde_json::from_str(json).ok()?;
            let name = value.get("name").and_then(Value::as_str).map(str::to_string);
            from_server_json(name.as_deref(), value)
        }
        ("claude", [mcp, add, name, json, ..]) if mcp == "mcp" && add == "add-json" => {
            from_server_json(Some(name), serde_json::from_str(json).ok()?)
        }
        ("codex", [mcp, add, args @ ..]) if mcp == "mcp" && add == "add" => codex_add(args),
        ("claude" | "gemini", [mcp, add, args @ ..]) if mcp == "mcp" && add == "add" => claude_add(args),
        _ => None,
    }
}

/// Options of `claude mcp add` / `gemini mcp add` that take a value the form has no field for.
const SKIPPED_VALUE_OPTIONS: &[&str] =
    &["-s", "--scope", "--client-id", "--callback-port", "--timeout", "--description", "--include-tools", "--exclude-tools"];

/// `claude|gemini mcp add [options] <name> <commandOrUrl> [args…]`; `--env` and `--header` take
/// several values each.
fn claude_add(words: &[String]) -> Option<McpServerExport> {
    let mut transport = None;
    let mut env = HashMap::new();
    let mut headers = HashMap::new();
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let (opt, inline) = split_option(&words[i]);
        i += 1;
        // Once the name and a command are known, the rest are the command's own arguments; after
        // a URL, options still follow (`… <url> --header "…"`).
        if (positional.len() >= 2 && !is_url(&positional[1])) || opt == "--" {
            positional.extend(words[i - usize::from(opt != "--")..].iter().cloned());
            break;
        }
        match opt {
            "-t" | "--transport" => transport = option_value(words, &mut i, inline),
            "-e" | "--env" => take_many(words, &mut i, inline, &mut env, assignment),
            "-H" | "--header" => take_many(words, &mut i, inline, &mut headers, header),
            o if SKIPPED_VALUE_OPTIONS.contains(&o) => {
                option_value(words, &mut i, inline);
            }
            o if o.starts_with('-') => {}
            _ => positional.push(words[i - 1].clone()),
        }
    }
    let mut positional = positional.into_iter();
    let name = positional.next()?;
    let target = positional.next()?;
    Some(server(&name, transport.as_deref(), target, positional.collect(), env, headers))
}

/// `codex mcp add <name> [--env K=V]… (--url <url> [--bearer-token-env-var VAR] | -- <command> [args…])`.
fn codex_add(words: &[String]) -> Option<McpServerExport> {
    let mut name = None;
    let mut url = None;
    let mut env = HashMap::new();
    let mut headers = HashMap::new();
    let mut command: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let (opt, inline) = split_option(&words[i]);
        i += 1;
        match opt {
            "--" => {
                command = words[i..].to_vec();
                break;
            }
            "--url" => url = option_value(words, &mut i, inline),
            "--env" => {
                if let Some((k, v)) = option_value(words, &mut i, inline).as_deref().and_then(assignment) {
                    env.insert(k, v);
                }
            }
            // Codex reads the token from this variable of its shell; Puppeteer needs the value.
            "--bearer-token-env-var" => {
                if let Some(var) = option_value(words, &mut i, inline) {
                    headers.insert("Authorization".to_string(), format!("Bearer ${{{var}}}"));
                }
            }
            o if o.starts_with('-') => {}
            _ if name.is_none() => name = Some(words[i - 1].clone()),
            _ => {
                command = words[i - 1..].to_vec();
                break;
            }
        }
    }
    let name = name?;
    let mut command = command.into_iter();
    let target = url.or_else(|| command.next())?;
    Some(server(&name, None, target, command.collect(), env, headers))
}

/// A server from a command line's parts: a URL target is remote (`http` unless told `sse`).
fn server(
    name: &str,
    transport: Option<&str>,
    target: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    headers: HashMap<String, String>,
) -> McpServerExport {
    let remote = is_url(&target);
    let transport = match transport {
        Some(t @ ("stdio" | "http" | "sse")) => t,
        _ if remote => "http",
        _ => "stdio",
    };
    McpServerExport {
        name: library_name(name),
        description: String::new(),
        transport: transport.to_string(),
        command: (!remote).then(|| target.clone()),
        args: if remote { Vec::new() } else { args },
        url: remote.then_some(target),
        env,
        headers,
        enabled: true,
    }
}

fn is_url(target: &str) -> bool {
    target.starts_with("http://") || target.starts_with("https://")
}

/// `--opt=value` as (`--opt`, `value`); anything else as itself.
fn split_option(word: &str) -> (&str, Option<String>) {
    match word.split_once('=') {
        Some((opt, value)) if opt.starts_with("--") => (opt, Some(value.to_string())),
        _ => (word, None),
    }
}

fn option_value(words: &[String], i: &mut usize, inline: Option<String>) -> Option<String> {
    if inline.is_some() {
        return inline;
    }
    let value = words.get(*i).cloned();
    *i += 1;
    value
}

/// An option's values: the inline one, then the next words as long as `parse` takes them.
fn take_many(
    words: &[String],
    i: &mut usize,
    inline: Option<String>,
    into: &mut HashMap<String, String>,
    parse: fn(&str) -> Option<(String, String)>,
) {
    if let Some((k, v)) = inline.as_deref().and_then(parse) {
        into.insert(k, v);
    }
    while let Some((k, v)) = words.get(*i).and_then(|w| parse(w)) {
        into.insert(k, v);
        *i += 1;
    }
}

/// `KEY=value`.
fn assignment(word: &str) -> Option<(String, String)> {
    let (key, value) = word.split_once('=')?;
    let valid = key.chars().next().is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && key.chars().all(|c| c == '_' || c.is_ascii_alphanumeric());
    valid.then(|| (key.to_string(), value.to_string()))
}

/// `Name: value`.
fn header(word: &str) -> Option<(String, String)> {
    let (key, value) = word.split_once(':')?;
    let valid = !key.is_empty() && key.chars().all(|c| c == '-' || c == '_' || c.is_ascii_alphanumeric());
    valid.then(|| (key.to_string(), value.trim().to_string()))
}

/// A shell line as words: `'…'` literal, `"…"` with `\` escapes, `\` escaping one character.
/// A leading prompt (`$ `, `> `) is dropped; `None` for an unclosed quote.
fn words(line: &str) -> Option<Vec<String>> {
    let line = line.trim();
    let line = line.strip_prefix("$ ").or_else(|| line.strip_prefix("> ")).unwrap_or(line);
    let mut out = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            '\'' => {
                started = true;
                loop {
                    match chars.next()? {
                        '\'' => break,
                        c => word.push(c),
                    }
                }
            }
            '"' => {
                started = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => {
                            let next = chars.next()?;
                            if !matches!(next, '"' | '\\' | '$' | '`') {
                                word.push('\\');
                            }
                            word.push(next);
                        }
                        c => word.push(c),
                    }
                }
            }
            '\\' => {
                started = true;
                word.extend(chars.next());
            }
            c => {
                started = true;
                word.push(c);
            }
        }
    }
    if started {
        out.push(word);
    }
    Some(out)
}

// ── Config blocks ───────────────────────────────────────────────────────────

/// The servers of a JSON (with comments) block: a whole config, its `"name": { … }` lines alone,
/// or one server.
fn from_json(block: &str) -> Vec<McpServerExport> {
    let parsed = [block.to_string(), format!("{{{block}}}")]
        .iter()
        .find_map(|raw| serde_json::from_str::<Value>(&strip_jsonc(raw)).ok().filter(Value::is_object));
    let Some(value) = parsed else { return Vec::new() };
    let servers = ["mcpServers", "servers"]
        .iter()
        .find_map(|key| value.get(key))
        .or_else(|| value.get("mcp").and_then(|mcp| mcp.get("servers")))
        .cloned();
    match servers {
        Some(Value::Object(servers)) => named_servers(servers),
        Some(_) => Vec::new(),
        None if is_server(&value) => {
            let name = value.get("name").and_then(Value::as_str).map(str::to_string);
            from_server_json(name.as_deref(), value).into_iter().collect()
        }
        None => match value {
            Value::Object(map) if !map.is_empty() && map.values().all(is_server) => named_servers(map),
            _ => Vec::new(),
        },
    }
}

fn named_servers(servers: serde_json::Map<String, Value>) -> Vec<McpServerExport> {
    servers.into_iter().filter_map(|(name, server)| from_server_json(Some(&name), server)).collect()
}

fn is_server(value: &Value) -> bool {
    ["command", "url", "serverUrl", "httpUrl"].iter().any(|key| value.get(key).is_some_and(Value::is_string))
}

/// One server in any tool's spelling, through the `mcpServers` reader. With no name, one is made
/// from its package or host.
fn from_server_json(name: Option<&str>, mut server: Value) -> Option<McpServerExport> {
    let obj = server.as_object_mut()?;
    // Windsurf's `serverUrl`, Gemini CLI's `httpUrl` (a plain `url` is SSE there, but most tools
    // mean streamable HTTP by it today).
    if let Some(url) = obj.remove("serverUrl").filter(Value::is_string) {
        obj.insert("url".into(), url);
    }
    if let Some(url) = obj.remove("httpUrl").filter(Value::is_string) {
        obj.insert("url".into(), url);
        obj.insert("type".into(), "http".into());
    }
    let transport = match obj.get("type").and_then(Value::as_str) {
        Some("local") => Some("stdio"),
        Some("streamable-http" | "streamableHttp" | "streamable_http" | "remote") => Some("http"),
        _ => None,
    };
    if let Some(t) = transport {
        obj.insert("type".into(), t.into());
    }
    // VS Code takes numbers too; null leaves a variable unset.
    if let Some(Value::Object(env)) = obj.get_mut("env") {
        env.retain(|_, v| !v.is_null());
        for v in env.values_mut() {
            if let Value::Number(n) = v {
                *v = Value::String(n.to_string());
            }
        }
    }
    let mut export = mcp_export("", &server).ok()?;
    export.name = library_name(&name.map(str::to_string).unwrap_or_else(|| guessed_name(&export)));
    Some(export)
}

/// The servers of a Codex `config.toml` block.
fn from_toml(block: &str) -> Vec<McpServerExport> {
    let Ok(table) = block.parse::<toml::Table>() else { return Vec::new() };
    let Some(toml::Value::Table(servers)) = table.get("mcp_servers") else { return Vec::new() };
    servers
        .iter()
        .filter_map(|(name, server)| {
            let mut server = server.as_table()?.clone();
            env_references_as_values(&mut server);
            super::codex::mcp_export(&library_name(name), &toml::Value::Table(server)).ok()
        })
        .collect()
}

/// Codex's keys that name a variable of its shell, as values naming it (`${VAR}`), so the draft
/// asks for them.
fn env_references_as_values(server: &mut toml::Table) {
    let mut headers = match server.remove("http_headers") {
        Some(toml::Value::Table(t)) => t,
        _ => toml::Table::new(),
    };
    let mut env = match server.remove("env") {
        Some(toml::Value::Table(t)) => t,
        _ => toml::Table::new(),
    };
    if let Some(toml::Value::String(var)) = server.remove("bearer_token_env_var") {
        headers.insert("Authorization".into(), format!("Bearer ${{{var}}}").into());
    }
    if let Some(toml::Value::Table(vars)) = server.remove("env_http_headers") {
        for (name, var) in vars {
            if let Some(var) = var.as_str() {
                headers.insert(name, format!("${{{var}}}").into());
            }
        }
    }
    if let Some(toml::Value::Array(vars)) = server.remove("env_vars") {
        for var in vars.iter().filter_map(toml::Value::as_str) {
            env.insert(var.into(), format!("${{{var}}}").into());
        }
    }
    if !headers.is_empty() {
        server.insert("http_headers".into(), headers.into());
    }
    if !env.is_empty() {
        server.insert("env".into(), env.into());
    }
}

// ── Drafts ──────────────────────────────────────────────────────────────────

/// A name from the server's package (`@notionhq/notion-mcp-server` → `notion-mcp-server`) or host
/// (`pms.example.com` → `pms`).
fn guessed_name(server: &McpServerExport) -> String {
    if let Some(url) = &server.url {
        let host = url.split("://").nth(1).unwrap_or(url).split(['/', ':']).next().unwrap_or_default();
        if let Some(label) = host.split('.').find(|l| !matches!(*l, "mcp" | "api" | "www")) {
            return label.to_string();
        }
    }
    let package = server.args.iter().find(|a| !a.starts_with('-')).or(server.command.as_ref());
    package
        .map(|p| {
            let p = p.rsplit('/').next().unwrap_or(p);
            // A version (`pkg@latest`), not a scope: the scope went with the `/`.
            p.split('@').next().unwrap_or(p).to_string()
        })
        .unwrap_or_default()
}

fn draft(server: McpServerExport) -> PastedServer {
    PastedServer {
        name: server.name,
        transport: server.transport,
        command: server.command,
        args: server.args,
        url: server.url,
        env: pasted_values(server.env),
        headers: pasted_values(server.headers),
    }
}

fn pasted_values(values: HashMap<String, String>) -> Vec<PastedValue> {
    let mut out: Vec<PastedValue> = values
        .into_iter()
        .map(|(key, value)| {
            let parts: Vec<&str> = value.split_whitespace().collect();
            if !parts.iter().any(|p| is_placeholder(p)) {
                return PastedValue { key, value, hint: None };
            }
            let hint: Vec<&str> = parts.iter().map(|p| if is_placeholder(p) { "…" } else { p }).collect();
            let hint = hint.join(" ");
            PastedValue { key, value: String::new(), hint: (hint != "…").then_some(hint) }
        })
        .collect();
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// Whether one word of a value only says what to put there.
fn is_placeholder(word: &str) -> bool {
    let var = word.strip_prefix('$').and_then(|r| r.chars().next()).is_some_and(|c| c == '{' || c == '_' || c.is_ascii_alphabetic());
    let angle = word.len() > 2 && word.starts_with('<') && word.ends_with('>');
    let lower = word.to_ascii_lowercase();
    var || angle
        || word.starts_with("{{")
        || lower.starts_with("your")
        || lower.contains("_your_")
        || matches!(word, "..." | "…")
        || (word.len() >= 3 && word.chars().all(|c| c == 'x' || c == 'X'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(key: &str, value: &str, hint: Option<&str>) -> PastedValue {
        PastedValue { key: key.into(), value: value.into(), hint: hint.map(str::to_string) }
    }

    fn one(text: &str) -> PastedServer {
        let mut servers = parse(text);
        assert_eq!(servers.len(), 1, "{servers:?}");
        servers.remove(0)
    }

    #[test]
    fn claude_add_with_a_bearer_placeholder_keeps_the_prefix_as_a_hint() {
        let s = one("claude mcp add --transport http pms https://pms.sora-techs.com/mcp --header \"Authorization: Bearer <token>\"");
        assert_eq!((s.name.as_str(), s.transport.as_str(), s.url.as_deref()), ("pms", "http", Some("https://pms.sora-techs.com/mcp")));
        assert_eq!(s.headers, [value("Authorization", "", Some("Bearer …"))]);
        assert_eq!(s.command, None);
    }

    #[test]
    fn claude_add_stdio_takes_env_values_and_the_command_after_the_dashes() {
        let s = one("$ claude mcp add -s user -e API_KEY=abc123 -e REGION=eu airtable -- npx -y airtable-mcp-server");
        assert_eq!((s.name.as_str(), s.transport.as_str(), s.command.as_deref()), ("airtable", "stdio", Some("npx")));
        assert_eq!(s.args, ["-y", "airtable-mcp-server"]);
        assert_eq!(s.env, [value("API_KEY", "abc123", None), value("REGION", "eu", None)]);
    }

    #[test]
    fn claude_add_without_dashes_and_a_continued_line() {
        let s = one("claude mcp add notion \\\n  npx -y @notionhq/notion-mcp-server --env NOTION_TOKEN=YOUR_TOKEN");
        assert_eq!(s.command.as_deref(), Some("npx"));
        // Past the name and the command, everything belongs to the command.
        assert_eq!(s.args, ["-y", "@notionhq/notion-mcp-server", "--env", "NOTION_TOKEN=YOUR_TOKEN"]);
    }

    #[test]
    fn gemini_add_reads_like_claude() {
        let s = one("gemini mcp add -t sse -H 'X-Api-Key: ${KEY}' ctx https://example.com/sse");
        assert_eq!((s.name.as_str(), s.transport.as_str()), ("ctx", "sse"));
        assert_eq!(s.headers, [value("X-Api-Key", "", None)]);
    }

    #[test]
    fn codex_add_remote_and_local() {
        let s = one("codex mcp add pms --url https://pms.example.com/mcp --bearer-token-env-var PMS_TOKEN");
        assert_eq!((s.transport.as_str(), s.url.as_deref()), ("http", Some("https://pms.example.com/mcp")));
        assert_eq!(s.headers, [value("Authorization", "", Some("Bearer …"))]);
        let s = one("codex mcp add context7 --env TOKEN=t0k -- npx -y @upstash/context7-mcp");
        assert_eq!((s.transport.as_str(), s.command.as_deref()), ("stdio", Some("npx")));
        assert_eq!(s.args, ["-y", "@upstash/context7-mcp"]);
        assert_eq!(s.env, [value("TOKEN", "t0k", None)]);
    }

    #[test]
    fn add_json_and_vscode_add_mcp() {
        let s = one(r#"claude mcp add-json weather '{"type":"stdio","command":"/bin/weather","args":["--api-key","k"]}'"#);
        assert_eq!((s.name.as_str(), s.command.as_deref()), ("weather", Some("/bin/weather")));
        let s = one(r#"code --add-mcp "{\"name\":\"my-server\",\"command\":\"uvx\",\"args\":[\"mcp-server-fetch\"]}""#);
        assert_eq!((s.name.as_str(), s.command.as_deref(), s.args.as_slice()), ("my-server", Some("uvx"), &["mcp-server-fetch".to_string()][..]));
    }

    #[test]
    fn json_configs_of_every_tool() {
        let s = one(r#"{"mcpServers":{"pms":{"type":"http","url":"https://pms.sora-techs.com/mcp","headers":{"Authorization":"Bearer ${PMS_ACCESS_TOKEN}"}}}}"#);
        assert_eq!((s.name.as_str(), s.transport.as_str()), ("pms", "http"));
        assert_eq!(s.headers, [value("Authorization", "", Some("Bearer …"))]);
        // VS Code, with a comment and a trailing comma.
        let s = one("{\n  // mine\n  \"servers\": { \"gh\": { \"type\": \"http\", \"url\": \"https://api.githubcopilot.com/mcp/\", }, },\n}");
        assert_eq!((s.name.as_str(), s.transport.as_str()), ("gh", "http"));
        // Windsurf's serverUrl, Gemini's httpUrl.
        assert_eq!(one(r#"{"mcpServers":{"w":{"serverUrl":"https://x.dev/mcp"}}}"#).url.as_deref(), Some("https://x.dev/mcp"));
        assert_eq!(one(r#"{"mcpServers":{"g":{"httpUrl":"https://x.dev/mcp"}}}"#).transport, "http");
        // VS Code settings.json.
        assert_eq!(one(r#"{"mcp":{"servers":{"v":{"command":"npx","env":{"PORT":3000}}}}}"#).env, [value("PORT", "3000", None)]);
    }

    #[test]
    fn json_fragments_and_a_lone_server() {
        let s = one(r#""slack": { "command": "npx", "args": ["-y", "slack-mcp-server@latest"] }"#);
        assert_eq!(s.name, "slack");
        let s = one(r#"{"command": "npx", "args": ["-y", "@notionhq/notion-mcp-server"]}"#);
        assert_eq!(s.name, "notion-mcp-server");
        assert_eq!(one(r#"{"url": "https://mcp.context7.com/mcp"}"#).name, "context7");
    }

    #[test]
    fn codex_toml_turns_env_references_into_values_to_fill() {
        let s = one("[mcp_servers.pms]\nurl = \"https://pms.example.com/mcp\"\nbearer_token_env_var = \"PMS_TOKEN\"\n\n[mcp_servers.pms.http_headers]\nX-Team = \"sora\"");
        assert_eq!((s.name.as_str(), s.transport.as_str()), ("pms", "http"));
        assert_eq!(s.headers, [value("Authorization", "", Some("Bearer …")), value("X-Team", "sora", None)]);
        let s = one("[mcp_servers.fs]\ncommand = \"npx\"\nargs = [\"-y\", \"server-fs\"]\nenv_vars = [\"HOME_DIR\"]\nenv = { MODE = \"ro\" }");
        assert_eq!(s.env, [value("HOME_DIR", "", None), value("MODE", "ro", None)]);
    }

    #[test]
    fn a_readme_is_read_by_its_fenced_blocks() {
        let readme = "# Foo MCP\n\nInstall:\n\n```bash\nnpm i -g foo\n```\n\nThen add to your config:\n\n```json\n{\"mcpServers\": {\"foo\": {\"command\": \"foo-mcp\"}}}\n```\n";
        assert_eq!(one(readme).command.as_deref(), Some("foo-mcp"));
        let prose = "Run this:\nclaude mcp add foo -- foo-mcp --verbose\nand restart.";
        assert_eq!(one(prose).args, ["--verbose"]);
    }

    #[test]
    fn text_without_a_known_format_gives_nothing() {
        assert!(parse("Go to settings and paste your token.").is_empty());
        assert!(parse("claude mcp list").is_empty());
        assert!(parse("claude mcp add \"unclosed").is_empty());
        assert!(parse("{\"theme\": \"dark\"}").is_empty());
    }

    #[test]
    fn placeholders_are_told_apart_from_values() {
        for p in ["${TOKEN}", "$TOKEN", "<token>", "YOUR_API_KEY", "your-key", "{{KEY}}", "xxxx", "...", "${env:KEY}"] {
            assert!(is_placeholder(p), "{p}");
        }
        for v in ["Bearer", "sk-live-123", "$5", "abc", "<>"] {
            assert!(!is_placeholder(v), "{v}");
        }
    }
}
