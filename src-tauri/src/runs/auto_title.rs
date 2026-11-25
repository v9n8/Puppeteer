//! Short session names for the sidebar / History list.
//!
//! A session starts out titled after its first prompt, cut to a sentence
//! (`runs.title_auto = 1`: the app chose it). When a turn ends, that title is
//! replaced, once, by a short name:
//! - Claude: the `ai-title` the CLI writes into the session transcript (a file
//!   read, no AI call);
//! - Codex, which names nothing itself: one short prompt on the warm side-task
//!   sidecar (`translate::submit_prompt`, like the grammar check), at most one
//!   per run at a time.
//!
//! Until a name exists (no `ai-title` yet, an AI error, a timeout, an empty
//! answer) the title stays, and the next turn end tries again.
//!
//! The write is a compare-and-set on `title_auto = 1`, so a name the user picks
//! while a name is being looked up always wins, and the first short name stays.

use crate::commands::sessions::{
    claude_sessions_dirs, pick_claude_transcript, truncate, PLACEHOLDER_TITLE, TITLE_MAX_CHARS,
};
use crate::commands::translate::{self, TranslateState, TurnEvent};
use crate::db::Db;
use serde_json::Value;
use std::collections::BTreeSet;
use std::future::Future;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc::UnboundedReceiver;

/// App event for a run's new automatic title: `{ run_id, title }`. Global, not
/// per run: the name can land after the run's own listeners stopped at `run:done`.
pub const EVENT: &str = "run:title";

/// How long a Codex session waits for its name before giving up until the next turn.
const NAMING_TIMEOUT: Duration = Duration::from_secs(30);

/// Longest name taken from an AI answer.
const NAME_MAX_CHARS: usize = 60;

/// Codex runs with a naming prompt in flight: a turn ending meanwhile sends no second one.
static NAMING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// After a turn of `run_id` ended: name the session if it still has an app-set
/// title. Spawned, so the sidecar read loop never waits on it.
pub fn refresh_later(app: &AppHandle, db: &Db, run_id: &str) {
    let (app, db, run_id) = (app.clone(), db.clone(), run_id.to_string());
    tauri::async_runtime::spawn(async move {
        let ask = |prompt: String| ask_side_task(&app, &db, prompt);
        if let Some(title) = retitle(&db, &run_id, ask).await {
            let _ = app.emit(EVENT, serde_json::json!({ "run_id": run_id, "title": title }));
        }
    });
}

/// A session whose title the app may still replace.
struct Candidate {
    engine: String,
    title: Option<String>,
    session_id: Option<String>,
    transcript_path: Option<String>,
    project_path: String,
}

async fn candidate(db: &Db, run_id: &str) -> Option<Candidate> {
    let row: Option<(String, Option<String>, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT r.engine, r.title, r.session_id, r.transcript_path, p.path FROM runs r JOIN projects p ON p.id = r.project_id
         WHERE r.id = ? AND r.type = 'session' AND r.title_auto = 1",
    )
    .bind(run_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();
    row.map(|(engine, title, session_id, transcript_path, project_path)| Candidate {
        engine,
        title,
        session_id,
        transcript_path,
        project_path,
    })
}

/// Replace the app-set title of `run_id` with a short name, if one is ready.
/// `ask` sends a Codex naming prompt and returns the model's answer. Returns the
/// stored name; `None` when nothing changed.
async fn retitle<A, F>(db: &Db, run_id: &str, ask: A) -> Option<String>
where
    A: FnOnce(String) -> F,
    F: Future<Output = Result<String, String>>,
{
    let c = candidate(db, run_id).await?;
    match c.engine.as_str() {
        "claude" => {
            let dirs = claude_sessions_dirs(db, &c.project_path).await;
            let title = claude_title(&dirs, c.session_id.as_deref()?, c.transcript_path.as_deref()).await?;
            replace(db, run_id, &title).await.then_some(title)
        }
        "codex" => {
            // Held until the name is stored, so a turn ending meanwhile neither
            // sends a second prompt nor races the write.
            let _naming = NamingGuard::claim(run_id)?;
            let title = codex_title(run_id, c.title.as_deref().unwrap_or(""), ask).await?;
            replace(db, run_id, &title).await.then_some(title)
        }
        _ => None,
    }
}

/// The last `ai-title` of the session's transcript (the copy the session sync
/// would follow), read on the blocking pool: a transcript can be tens of MB.
async fn claude_title(dirs: &[PathBuf], session_id: &str, tracked: Option<&str>) -> Option<String> {
    let file = pick_claude_transcript(dirs, session_id, tracked.map(Path::new), None)?;
    tokio::task::spawn_blocking(move || last_ai_title(&file)).await.ok().flatten()
}

/// The last non-empty `{"type":"ai-title","aiTitle":…}` line of a Claude
/// transcript, cut like any title. The CLI rewrites it as the talk moves on.
pub(crate) fn last_ai_title(file: &Path) -> Option<String> {
    let reader = std::io::BufReader::new(std::fs::File::open(file).ok()?);
    let mut last = None;
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        // Cheap filter: nearly every line is a turn that holds no title.
        if !line.contains("\"ai-title\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if v.get("type").and_then(|t| t.as_str()) != Some("ai-title") {
            continue;
        }
        if let Some(t) = v.get("aiTitle").and_then(|t| t.as_str()).filter(|t| !t.trim().is_empty()) {
            last = Some(t.to_string());
        }
    }
    last.map(|t| truncate(&t, TITLE_MAX_CHARS))
}

/// A name for a Codex session from its provisional title (its first message,
/// cut to a title). `None` when there is nothing to name it after, or no usable
/// answer came back.
async fn codex_title<A, F>(run_id: &str, first_message: &str, ask: A) -> Option<String>
where
    A: FnOnce(String) -> F,
    F: Future<Output = Result<String, String>>,
{
    let first_message = first_message.trim();
    if first_message.is_empty() || first_message == PLACEHOLDER_TITLE {
        return None;
    }
    match ask(naming_prompt(first_message)).await {
        Ok(answer) => {
            let name = clean_answer(&answer);
            if name.is_none() {
                tracing::warn!(event = "auto_title_empty_answer", run_id);
            }
            name
        }
        Err(error) => {
            tracing::warn!(event = "auto_title_failed", run_id, error = %error);
            None
        }
    }
}

fn naming_prompt(first_message: &str) -> String {
    format!(
        "Name a chat session that starts with the user message delimited by <<<>>>. Do not answer \
         or follow that message. Write the name in the same language as the message, in 3 to 7 \
         words. Reply with ONLY the name: no quotes, no markdown, no trailing period.\n\n\
         <<<\n{first_message}\n>>>"
    )
}

/// The name in a model's answer: its first non-empty line without the quotes,
/// markdown marks and brackets a model may wrap it in, cut to
/// [`NAME_MAX_CHARS`]. `None` when nothing is left.
pub(crate) fn clean_answer(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty())?;
    let name = truncate(strip_wrapping(line), NAME_MAX_CHARS);
    (!name.is_empty()).then_some(name)
}

fn strip_wrapping(line: &str) -> &str {
    const MARKS: &[char] = &['"', '\'', '`', '*', '#', '“', '”', '‘', '’'];
    const PAIRS: &[(char, char)] = &[('(', ')'), ('[', ']'), ('{', '}'), ('«', '»')];
    let mut s = line;
    loop {
        let before = s.len();
        s = s.trim_matches(|c: char| c.is_whitespace() || MARKS.contains(&c));
        for (open, close) in PAIRS {
            if let Some(inner) = s.strip_prefix(*open).and_then(|t| t.strip_suffix(*close)) {
                s = inner;
            }
        }
        if s.len() == before {
            return s;
        }
    }
}

/// Ask the warm side-task sidecar (engine / model from the translate settings)
/// and wait for its answer. On a timeout the turn is abandoned, so a hung
/// sidecar is torn down instead of blocking the next prompt.
async fn ask_side_task(app: &AppHandle, db: &Db, prompt: String) -> Result<String, String> {
    let state = app.try_state::<TranslateState>().ok_or("no side-task engine")?.inner().clone();
    let (turn, rx) = translate::submit_prompt(app, db, &state, prompt, true).await?;
    let rx = rx.ok_or("no result channel")?;
    match wait_answer(rx, NAMING_TIMEOUT).await {
        Some(answer) => answer,
        None => {
            state.abandon_turn(turn).await;
            Err(format!("timed out after {}s", NAMING_TIMEOUT.as_secs()))
        }
    }
}

/// A turn's final text or error; `None` when `limit` passed first.
async fn wait_answer(mut rx: UnboundedReceiver<TurnEvent>, limit: Duration) -> Option<Result<String, String>> {
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
    tokio::time::timeout(limit, wait).await.ok()
}

/// A run's place in [`NAMING`], given back when dropped (answer, error,
/// timeout or a panic alike).
struct NamingGuard(String);

impl NamingGuard {
    /// `None` when `run_id` already has a naming prompt in flight.
    fn claim(run_id: &str) -> Option<Self> {
        let mut set = NAMING.lock().unwrap_or_else(|e| e.into_inner());
        set.insert(run_id.to_string()).then(|| NamingGuard(run_id.to_string()))
    }
}

impl Drop for NamingGuard {
    fn drop(&mut self) {
        NAMING.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.0);
    }
}

/// Store an automatic title, only over an app-set one (compare-and-set), and
/// mark it final. `true` when the row changed.
pub(crate) async fn replace(db: &Db, run_id: &str, title: &str) -> bool {
    let res = sqlx::query("UPDATE runs SET title = ?, title_auto = 0 WHERE id = ? AND title_auto = 1")
        .bind(title)
        .bind(run_id)
        .execute(db)
        .await;
    match res {
        Ok(r) => r.rows_affected() == 1,
        Err(e) => {
            tracing::warn!(event = "auto_title_write_failed", run_id, error = %e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::cell::RefCell;

    async fn db() -> Db {
        let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'p', '/nowhere/auto-title', 'c')")
            .execute(&db)
            .await
            .unwrap();
        db
    }

    /// A run `id` of `kind` on `engine` with the given title state.
    async fn run(db: &Db, id: &str, kind: &str, engine: &str, title: &str, title_auto: i64, transcript: Option<&Path>) {
        sqlx::query(
            "INSERT INTO runs (id, project_id, type, status, engine, session_id, transcript_path, title, title_auto, created_at)
             VALUES (?, 'p1', ?, 'running', ?, ?, ?, ?, ?, 'c')",
        )
        .bind(id)
        .bind(kind)
        .bind(engine)
        .bind(format!("s-{id}"))
        .bind(transcript.map(|p| p.to_string_lossy().to_string()))
        .bind(title)
        .bind(title_auto)
        .execute(db)
        .await
        .unwrap();
    }

    async fn title_of(db: &Db, id: &str) -> (String, i64) {
        sqlx::query_as("SELECT title, title_auto FROM runs WHERE id = ?").bind(id).fetch_one(db).await.unwrap()
    }

    /// The Codex `ask` of a run that must not send a naming prompt.
    fn no_ask(prompt: String) -> std::future::Ready<Result<String, String>> {
        panic!("no naming prompt expected, got: {prompt}")
    }

    /// A Codex `ask` that records the prompt and returns `answer`.
    fn answering<'a>(
        answer: Result<&'a str, &'a str>,
        prompts: &'a RefCell<Vec<String>>,
    ) -> impl FnOnce(String) -> std::future::Ready<Result<String, String>> + 'a {
        move |prompt| {
            prompts.borrow_mut().push(prompt);
            std::future::ready(answer.map(str::to_string).map_err(str::to_string))
        }
    }

    /// A run id no other test uses: the in-flight set is process-wide.
    fn fresh_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("puppeteer-auto-title-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ai_title(t: &str) -> String {
        serde_json::json!({ "type": "ai-title", "aiTitle": t, "sessionId": "s" }).to_string()
    }

    fn user(text: &str) -> String {
        serde_json::json!({ "type": "user", "cwd": "/x", "message": { "role": "user", "content": text } }).to_string()
    }

    #[test]
    fn the_last_ai_title_wins_and_blank_ones_are_skipped() {
        let dir = temp_dir();
        let file = dir.join("s.jsonl");
        let lines = [user("a prompt about \"ai-title\""), ai_title("First"), user("more"), ai_title("Second"), ai_title("  ")];
        std::fs::write(&file, lines.join("\n")).unwrap();
        assert_eq!(last_ai_title(&file).as_deref(), Some("Second"));

        std::fs::write(&file, user("no title yet")).unwrap();
        assert_eq!(last_ai_title(&file), None);
        assert_eq!(last_ai_title(&dir.join("missing.jsonl")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_claude_session_takes_its_transcript_title_once() {
        let db = db().await;
        let dir = temp_dir();
        let file = dir.join("s-r1.jsonl");
        std::fs::write(&file, user("Explain the usage guard hook")).unwrap();
        run(&db, "r1", "session", "claude", "Explain the usage guard hook", 1, Some(&file)).await;

        // No ai-title yet: nothing changes, the next turn end looks again.
        assert_eq!(retitle(&db, "r1", no_ask).await, None);
        assert_eq!(title_of(&db, "r1").await, ("Explain the usage guard hook".into(), 1));

        std::fs::write(&file, [user("Explain"), ai_title("First"), ai_title("Usage guard hook")].join("\n")).unwrap();
        assert_eq!(retitle(&db, "r1", no_ask).await.as_deref(), Some("Usage guard hook"));
        assert_eq!(title_of(&db, "r1").await, ("Usage guard hook".into(), 0));

        // Replaced once: a newer ai-title later does not move it again.
        std::fs::write(&file, ai_title("Something else")).unwrap();
        assert_eq!(retitle(&db, "r1", no_ask).await, None);
        assert_eq!(title_of(&db, "r1").await.0, "Usage guard hook");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn only_app_titled_sessions_are_renamed() {
        let db = db().await;
        let dir = temp_dir();
        let file = dir.join("t.jsonl");
        std::fs::write(&file, ai_title("Short name")).unwrap();
        // A user / Focus AI / pre-migration title, and a review run.
        run(&db, "mine", "session", "claude", "Mine", 0, Some(&file)).await;
        run(&db, "review", "review_pr", "claude", "PR #1", 1, Some(&file)).await;
        assert_eq!(retitle(&db, "mine", no_ask).await, None);
        assert_eq!(retitle(&db, "review", no_ask).await, None);
        assert_eq!(title_of(&db, "mine").await, ("Mine".into(), 0));
        assert_eq!(title_of(&db, "review").await, ("PR #1".into(), 1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_transcript_is_found_by_session_id_when_no_path_is_stored() {
        let dir = temp_dir();
        std::fs::write(dir.join("s-r1.jsonl"), ai_title("Found by id")).unwrap();
        assert_eq!(claude_title(&[dir.clone()], "s-r1", None).await.as_deref(), Some("Found by id"));
        assert_eq!(claude_title(&[dir.clone()], "s-other", None).await, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_automatic_write_never_overwrites_a_name_the_user_chose() {
        let db = db().await;
        run(&db, "r1", "session", "claude", "Mine", 0, None).await;
        assert!(!replace(&db, "r1", "Automatic").await);
        assert_eq!(title_of(&db, "r1").await, ("Mine".into(), 0));

        run(&db, "r2", "session", "claude", "Provisional", 1, None).await;
        assert!(replace(&db, "r2", "Automatic").await);
        assert!(!replace(&db, "r2", "Again").await);
        assert_eq!(title_of(&db, "r2").await, ("Automatic".into(), 0));
    }

    #[test]
    fn an_answer_is_cut_down_to_the_name() {
        assert_eq!(clean_answer("Cơ chế usage-guard hook").as_deref(), Some("Cơ chế usage-guard hook"));
        assert_eq!(
            clean_answer("\"# **Cơ chế usage-guard hook**\"\nThis name fits because…").as_deref(),
            Some("Cơ chế usage-guard hook")
        );
        assert_eq!(clean_answer("\n  “Tài khoản phụ”  \n").as_deref(), Some("Tài khoản phụ"));
        assert_eq!(clean_answer("(`Login fix`)").as_deref(), Some("Login fix"));
        // Inner quotes and brackets stay.
        assert_eq!(clean_answer("Fix the user's (old) login").as_deref(), Some("Fix the user's (old) login"));
        let long = "Refactor the session title logic so it keeps handling very long prompts well";
        assert_eq!(clean_answer(&format!("**{long}**")), Some(truncate(long, NAME_MAX_CHARS)));
        for empty in ["", "  \n \n", "\"\"", "** ##", "()"] {
            assert_eq!(clean_answer(empty), None, "{empty:?}");
        }
    }

    #[test]
    fn the_naming_prompt_carries_the_message_and_the_rules() {
        let p = naming_prompt("Giải thích cơ chế usage-guard hook");
        assert!(p.contains("<<<\nGiải thích cơ chế usage-guard hook\n>>>"), "{p}");
        assert!(p.contains("same language") && p.contains("3 to 7 words"), "{p}");
        assert!(p.contains("no quotes") && p.contains("no trailing period"), "{p}");
    }

    #[tokio::test]
    async fn a_codex_session_is_named_from_its_first_message() {
        let db = db().await;
        let id = fresh_id();
        run(&db, &id, "session", "codex", "Giải thích cơ chế usage-guard hook", 1, None).await;
        let prompts = RefCell::new(Vec::new());
        let named = retitle(&db, &id, answering(Ok("Cơ chế usage-guard hook"), &prompts)).await;
        assert_eq!(named.as_deref(), Some("Cơ chế usage-guard hook"));
        assert_eq!(title_of(&db, &id).await, ("Cơ chế usage-guard hook".into(), 0));
        assert_eq!(prompts.borrow().len(), 1);
        assert!(prompts.borrow()[0].contains("Giải thích cơ chế usage-guard hook"));
        // Named once: later turns send nothing.
        assert_eq!(retitle(&db, &id, no_ask).await, None);
    }

    #[tokio::test]
    async fn a_failed_or_empty_answer_keeps_the_title_and_the_next_turn_retries() {
        let db = db().await;
        let id = fresh_id();
        run(&db, &id, "session", "codex", "Fix the login bug", 1, None).await;
        let prompts = RefCell::new(Vec::new());
        for answer in [Err("timed out after 30s"), Err("engine crashed"), Ok("**  **"), Ok("")] {
            assert_eq!(retitle(&db, &id, answering(answer, &prompts)).await, None);
            assert_eq!(title_of(&db, &id).await, ("Fix the login bug".into(), 1));
        }
        // Each turn end asked again: the guard was given back every time.
        assert_eq!(prompts.borrow().len(), 4);
        assert_eq!(retitle(&db, &id, answering(Ok("Login bug fix"), &prompts)).await.as_deref(), Some("Login bug fix"));
    }

    #[tokio::test]
    async fn nothing_to_name_it_after_sends_no_prompt() {
        let db = db().await;
        let (blank, placeholder) = (fresh_id(), fresh_id());
        run(&db, &blank, "session", "codex", "   ", 1, None).await;
        run(&db, &placeholder, "session", "codex", PLACEHOLDER_TITLE, 1, None).await;
        assert_eq!(retitle(&db, &blank, no_ask).await, None);
        assert_eq!(retitle(&db, &placeholder, no_ask).await, None);
        assert_eq!(title_of(&db, &placeholder).await, (PLACEHOLDER_TITLE.into(), 1));
    }

    #[tokio::test]
    async fn one_naming_prompt_per_run_at_a_time() {
        let db = db().await;
        let (busy, other) = (fresh_id(), fresh_id());
        run(&db, &busy, "session", "codex", "First task", 1, None).await;
        run(&db, &other, "session", "codex", "Second task", 1, None).await;
        let in_flight = NamingGuard::claim(&busy).unwrap();
        assert!(NamingGuard::claim(&busy).is_none());
        // A turn of the same run ends meanwhile: no second prompt.
        assert_eq!(retitle(&db, &busy, no_ask).await, None);
        // Another run is not held up.
        let prompts = RefCell::new(Vec::new());
        assert_eq!(retitle(&db, &other, answering(Ok("Second"), &prompts)).await.as_deref(), Some("Second"));
        drop(in_flight);
        assert_eq!(retitle(&db, &busy, answering(Ok("First"), &prompts)).await.as_deref(), Some("First"));
        assert_eq!(prompts.borrow().len(), 2);
    }

    #[tokio::test]
    async fn a_rename_while_the_name_is_on_its_way_wins() {
        let db = db().await;
        let id = fresh_id();
        run(&db, &id, "session", "codex", "Fix the login bug", 1, None).await;
        let ask = |_prompt: String| {
            let (db, id) = (db.clone(), id.clone());
            async move {
                sqlx::query("UPDATE runs SET title = 'Mine', title_auto = 0 WHERE id = ?").bind(&id).execute(&db).await.unwrap();
                Ok("Login bug fix".to_string())
            }
        };
        assert_eq!(retitle(&db, &id, ask).await, None);
        assert_eq!(title_of(&db, &id).await, ("Mine".into(), 0));
        assert!(NamingGuard::claim(&id).is_some(), "the guard is given back");
    }

    #[tokio::test]
    async fn waiting_for_an_answer_ends_on_done_error_or_the_time_limit() {
        let limit = Duration::from_millis(50);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tx.send(TurnEvent::Chunk("Log".into())).unwrap();
        tx.send(TurnEvent::Done("Login fix".into())).unwrap();
        assert_eq!(wait_answer(rx, limit).await, Some(Ok("Login fix".into())));

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tx.send(TurnEvent::Error("boom".into())).unwrap();
        assert_eq!(wait_answer(rx, limit).await, Some(Err("boom".into())));

        // A sidecar that never answers: the caller abandons the turn.
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel::<TurnEvent>();
        assert_eq!(wait_answer(rx, limit).await, None);
    }
}
