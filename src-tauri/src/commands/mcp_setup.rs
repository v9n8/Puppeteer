//! "Add with AI" on the MCP screen (UX-86): pasted setup text, or a link to a README, as drafts
//! for the MCP form. Text a known format matches is read without the AI ([`paste`]). Only the
//! rest goes to the side-task engine, which answers with an `mcpServers` block that the same
//! reader turns into drafts. Nothing is saved here: the user fills the secrets, tests and saves
//! in the form.
//!
//! Secrets stay out of the AI's hands both ways: credentials in the text are redacted before the
//! prompt ([`redact`]), and every env / header VALUE of an AI draft is dropped (its hint, e.g.
//! `Bearer …`, is kept), so a value the model made up never reaches the form.

use crate::commands::translate::{submit_prompt_on, SideTaskTarget, TranslateState, TurnEvent};
use crate::db::Db;
use crate::import::paste::{self, PastedServer};
use crate::runs::transcript::redact;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, State};

/// The model has this long to answer.
const TURN_TIMEOUT: Duration = Duration::from_secs(120);
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
/// A README bigger than this is cut before it is read.
const MAX_FETCH_BYTES: usize = 512 * 1024;
/// What of the text the AI reads; setup steps sit near the top of a README.
const MAX_PROMPT_CHARS: usize = 40_000;

/// The side-task engine of "Add with AI": its own warm instance, so a request never queues behind
/// translations, and a fresh session per request (one README never leaks into the next).
#[derive(Clone)]
pub struct McpSetupState(TranslateState);

impl Default for McpSetupState {
    fn default() -> Self {
        McpSetupState(
            TranslateState::default()
                .with_recycle_after(1)
                .silent()
                .tracked_as(crate::procs::ProcKind::McpSetup),
        )
    }
}

impl McpSetupState {
    /// Stop the warm session (app exit).
    pub async fn shutdown(&self) {
        self.0.shutdown().await;
    }
}

#[derive(Debug, Serialize)]
pub struct McpSetupDrafts {
    pub servers: Vec<PastedServer>,
    /// Read by the AI rather than a known format: worth a closer look before saving.
    pub by_ai: bool,
}

/// Drafts from pasted setup text or a README link: a known format first, the AI otherwise, on
/// the engine / model / account picked in the dialog (empty model or account = that engine's
/// default). No server found is an empty list, not an error.
#[tauri::command]
pub async fn draft_mcp_servers(
    app: AppHandle,
    db: State<'_, Db>,
    state: State<'_, McpSetupState>,
    text: String,
    engine: String,
    model: String,
    account_id: Option<String>,
) -> Result<McpSetupDrafts, String> {
    if text.len() > paste::MAX_TEXT_BYTES {
        return Err("The pasted text is too long".to_string());
    }
    let text = match readme_url(text.trim()) {
        Some(url) => fetch_text(&url).await?,
        None => text,
    };
    let servers = paste::parse(&text);
    if !servers.is_empty() {
        return Ok(McpSetupDrafts { servers, by_ai: false });
    }
    let target = SideTaskTarget { engine: &engine, model: &model, account_id: account_id.as_deref() };
    let answer = ask(&app, db.inner(), &state.0, prompt(&text), target).await?;
    Ok(McpSetupDrafts { servers: ai_drafts(&answer), by_ai: true })
}

async fn ask(
    app: &AppHandle,
    db: &Db,
    state: &TranslateState,
    prompt: String,
    target: SideTaskTarget<'_>,
) -> Result<String, String> {
    let (turn, rx) = submit_prompt_on(app, db, state, prompt, target).await?;
    let mut rx = rx.ok_or("no result channel")?;
    let wait = async {
        loop {
            match rx.recv().await {
                Some(TurnEvent::Chunk(_)) => continue,
                Some(TurnEvent::Done(text)) => return Ok(text),
                Some(TurnEvent::Error(e)) => return Err(e),
                None => return Err("the AI engine stopped unexpectedly".to_string()),
            }
        }
    };
    match tokio::time::timeout(TURN_TIMEOUT, wait).await {
        Ok(r) => r,
        Err(_) => {
            state.abandon_turn(turn).await;
            Err(format!("the AI did not answer within {}s", TURN_TIMEOUT.as_secs()))
        }
    }
}

fn prompt(text: &str) -> String {
    let text = redact(text);
    let text: String = text.chars().take(MAX_PROMPT_CHARS).collect();
    format!(
        "You turn setup instructions for MCP (Model Context Protocol) servers into a config. Read \
         the text between <text> tags (a README, a web page or notes) and reply with ONLY a JSON \
         object, no markdown fences and no comments:\n\
         {{\"mcpServers\": {{\"<name>\": {{\"type\": \"stdio\", \"command\": \"<program>\", \"args\": [\"<arg>\"], \"env\": {{\"<VAR>\": \"${{VAR}}\"}}}}}}}}\n\
         or, for a server reached over the network:\n\
         {{\"mcpServers\": {{\"<name>\": {{\"type\": \"http\", \"url\": \"<https url>\", \"headers\": {{\"<Header-Name>\": \"${{VAR}}\"}}}}}}}}\n\
         Rules:\n\
         - Only servers the text says how to run. Prefer the npx / uvx / docker way it gives for \
         a local server, and \"sse\" as the type only when the text says SSE. None found: \
         {{\"mcpServers\": {{}}}}.\n\
         - <name>: short, lowercase, letters, digits and dashes (the product name, e.g. \"github\").\n\
         - Never write a secret or a real value for env vars and headers: always \"${{VAR}}\" with \
         the variable named after what it holds. When a header has fixed text around the secret, \
         keep it: \"Authorization\": \"Bearer ${{API_TOKEN}}\".\n\
         - Leave out env vars the text marks as optional.\n\n\
         <text>\n{text}\n</text>"
    )
}

/// The servers of the model's answer, without any env / header value it wrote.
fn ai_drafts(answer: &str) -> Vec<PastedServer> {
    let (Some(start), Some(end)) = (answer.find('{'), answer.rfind('}')) else { return Vec::new() };
    if end < start {
        return Vec::new();
    }
    let mut servers = paste::parse(&answer[start..=end]);
    for server in &mut servers {
        for v in server.env.iter_mut().chain(server.headers.iter_mut()) {
            v.value.clear();
        }
    }
    servers.retain(|s| s.url.as_deref().is_none_or(|u| u.starts_with("https://") || u.starts_with("http://")));
    servers
}

/// The URL to read when `text` is only a link: GitHub pages as their raw README, an npm package
/// as its registry entry, anything else as is.
fn readme_url(text: &str) -> Option<String> {
    if text.contains(char::is_whitespace) || !text.starts_with("https://") {
        return None;
    }
    let path = |prefix: &str| text.strip_prefix(prefix).map(|p| p.trim_end_matches('/').split('#').next().unwrap_or_default().to_string());
    if let Some(p) = path("https://github.com/") {
        let parts: Vec<&str> = p.split('/').collect();
        return Some(match parts.as_slice() {
            [owner, repo] => format!("https://raw.githubusercontent.com/{owner}/{repo}/HEAD/README.md"),
            [owner, repo, "blob", rest @ ..] => format!("https://raw.githubusercontent.com/{owner}/{repo}/{}", rest.join("/")),
            [owner, repo, "tree", rest @ ..] => format!("https://raw.githubusercontent.com/{owner}/{repo}/{}/README.md", rest.join("/")),
            _ => text.to_string(),
        });
    }
    if let Some(name) = path("https://www.npmjs.com/package/").or_else(|| path("https://npmjs.com/package/")) {
        return Some(format!("https://registry.npmjs.org/{name}/latest"));
    }
    Some(text.to_string())
}

/// The text of `url`: an npm registry entry's README, a page without its markup, or as is.
async fn fetch_text(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder().timeout(FETCH_TIMEOUT).build().map_err(|e| e.to_string())?;
    let res = client.get(url).send().await.map_err(|e| format!("Could not open the link: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("Could not open the link (HTTP {})", res.status().as_u16()));
    }
    let html = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("html"));
    let bytes = res.bytes().await.map_err(|e| format!("Could not read the link: {e}"))?;
    let body = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_FETCH_BYTES)]).into_owned();
    if url.starts_with("https://registry.npmjs.org/") {
        let readme = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("readme").and_then(|r| r.as_str()).map(str::to_string));
        return readme.ok_or_else(|| "The npm package has no README".to_string());
    }
    Ok(if html { page_text(&body) } else { body })
}

/// An HTML page as text: scripts, styles and tags dropped, the common entities decoded.
fn page_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    loop {
        let Some(open) = rest.find('<') else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..open]);
        let tag = &rest[open..];
        let head: String = tag.chars().take(8).collect::<String>().to_ascii_lowercase();
        let close = if head.starts_with("<script") {
            "</script>"
        } else if head.starts_with("<style") {
            "</style>"
        } else {
            ">"
        };
        let Some(end) = find_ignoring_case(tag, close) else { break };
        rest = &tag[end + close.len()..];
        // Block tags end a line, so a code block's lines stay apart.
        if close != ">" || ["<br", "<p", "</p", "<div", "</div", "<pre", "</pre", "<li", "<h"].iter().any(|t| head.starts_with(t)) {
            out.push('\n');
        }
    }
    out.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&nbsp;", " ").replace("&amp;", "&")
}

/// Where ASCII `needle` starts in `hay`, whatever the case.
fn find_ignoring_case(hay: &str, needle: &str) -> Option<usize> {
    hay.as_bytes().windows(needle.len()).position(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_read_as_their_readme() {
        assert_eq!(readme_url("https://github.com/upstash/context7").as_deref(), Some("https://raw.githubusercontent.com/upstash/context7/HEAD/README.md"));
        assert_eq!(
            readme_url("https://github.com/a/b/tree/main/packages/mcp/").as_deref(),
            Some("https://raw.githubusercontent.com/a/b/main/packages/mcp/README.md")
        );
        assert_eq!(readme_url("https://github.com/a/b/blob/main/docs/mcp.md#setup").as_deref(), Some("https://raw.githubusercontent.com/a/b/main/docs/mcp.md"));
        assert_eq!(readme_url("https://www.npmjs.com/package/@notionhq/notion-mcp-server").as_deref(), Some("https://registry.npmjs.org/@notionhq/notion-mcp-server/latest"));
        assert_eq!(readme_url("https://docs.example.com/mcp").as_deref(), Some("https://docs.example.com/mcp"));
        // Text with a link in it, and plain http, are text.
        assert_eq!(readme_url("see https://github.com/a/b"), None);
        assert_eq!(readme_url("http://intranet/mcp"), None);
    }

    #[test]
    fn the_prompt_carries_no_credentials() {
        let p = prompt("Set Authorization: Bearer abc.def-1234567890XYZ and SLACK_TOKEN=xoxb-1234567890-abcdefghijkl");
        assert!(!p.contains("1234567890XYZ") && !p.contains("xoxb-"), "{p}");
        assert!(p.contains("<text>\nSet Authorization: Bearer [redacted]"));
    }

    #[test]
    fn ai_drafts_keep_keys_and_hints_but_no_values() {
        let answer = "Here you go:\n```json\n{\"mcpServers\": {\"acme\": {\"type\": \"http\", \"url\": \"https://mcp.acme.dev/mcp\", \"headers\": {\"Authorization\": \"Bearer ${ACME_TOKEN}\", \"X-Org\": \"made-up-org\"}}}}\n```";
        let servers = ai_drafts(answer);
        assert_eq!(servers.len(), 1);
        let h = &servers[0].headers;
        assert_eq!((h[0].key.as_str(), h[0].value.as_str(), h[0].hint.as_deref()), ("Authorization", "", Some("Bearer …")));
        assert_eq!((h[1].key.as_str(), h[1].value.as_str()), ("X-Org", ""));
        assert!(ai_drafts("{\"mcpServers\": {}}").is_empty());
        assert!(ai_drafts("I could not find any server.").is_empty());
        assert!(ai_drafts("{\"mcpServers\": {\"x\": {\"url\": \"file:///etc/passwd\"}}}").is_empty());
    }

    #[test]
    fn pages_lose_their_markup_but_keep_code_lines_apart() {
        let html = "<html><head><style>p{}</style><script>var a='<b>'</script></head><body><h1>Foo</h1><pre><code>claude mcp add foo -- npx -y foo-mcp\n</code></pre><p>a &amp; b &lt;token&gt;</p></body></html>";
        let text = page_text(html);
        assert!(text.contains("claude mcp add foo -- npx -y foo-mcp"), "{text}");
        assert!(text.contains("a & b <token>"));
        assert!(!text.contains("var a") && !text.contains("p{}"));
        assert_eq!(paste::parse(&text)[0].name, "foo");
    }
}
