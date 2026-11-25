//! Session titles on the import / sync path (`sessions::upsert_session_run_core`).
//!
//! A mirrored session is titled from its transcript: the last `ai-title` the
//! Claude CLI wrote, else its first message, else its first slash command with
//! arguments, else "Imported session". Any title but an `ai-title` is app-set
//! (`runs.title_auto = 1`), so a later sync replaces it once with the
//! transcript's `ai-title`, as `runs::auto_title` does at a live turn end.

use super::sessions::{truncate, ParsedTranscript, PLACEHOLDER_TITLE, TITLE_MAX_CHARS};
use crate::db::Db;

/// The title of a bare slash-command message (`<command-name>/us</command-name>`
/// and `<command-args>…</command-args>`, in either order): `"/us <args>"`.
/// `None` for `/clear` and for a command without arguments: local commands
/// (`/usage`, `/compact`, `/model`…) say nothing about the session.
pub(crate) fn slash_command_title(text: &str) -> Option<String> {
    let name = tag(text, "command-name")?.trim();
    let args = tag(text, "command-args").unwrap_or("").trim();
    if name.is_empty() || args.is_empty() || name == "/clear" {
        return None;
    }
    let name = if name.starts_with('/') { name.to_string() } else { format!("/{name}") };
    Some(truncate(&format!("{name} {args}"), TITLE_MAX_CHARS))
}

/// The text inside the first `<name>…</name>` of `text`.
fn tag<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let start = text.find(&open)? + open.len();
    let end = start + text[start..].find(&format!("</{name}>"))?;
    Some(&text[start..end])
}

/// An "Imported session" the app may still name: its transcript is read once
/// more even when it did not grow ([`repair_placeholder`]).
pub(crate) fn needs_repair(title: Option<&str>, title_auto: i64) -> bool {
    title_auto == 1 && title == Some(PLACEHOLDER_TITLE)
}

/// After a sync parsed the transcript of an existing run with `title` /
/// `title_auto`: the transcript's `ai-title` replaces an app-set title, once;
/// without one, an "Imported session" takes the parse's better title and stays
/// app-set. Compare-and-set, so a rename made meanwhile wins. `true` when the
/// title changed.
pub(crate) async fn apply_synced_title(
    db: &Db,
    run_id: &str,
    title: Option<&str>,
    title_auto: i64,
    parsed: &ParsedTranscript,
) -> bool {
    if title_auto != 1 {
        return false;
    }
    if let Some(ai_title) = &parsed.ai_title {
        return crate::runs::auto_title::replace(db, run_id, ai_title).await;
    }
    if title != Some(PLACEHOLDER_TITLE) || parsed.title == PLACEHOLDER_TITLE {
        return false;
    }
    let res = sqlx::query("UPDATE runs SET title = ? WHERE id = ? AND title_auto = 1 AND title = ?")
        .bind(&parsed.title)
        .bind(run_id)
        .bind(PLACEHOLDER_TITLE)
        .execute(db)
        .await;
    changed_one(run_id, res)
}

/// The one-time repair of an "Imported session" whose transcript did not grow
/// ([`needs_repair`]); `parsed` is that transcript, `None` when it holds no
/// conversation. A title it yields now replaces the placeholder; otherwise the
/// run keeps it and stops being app-set, so the same file is not read again.
/// Only `title` / `title_auto` change: an unchanged transcript has nothing new
/// for the log or the usage. `true` when the title changed.
pub(crate) async fn repair_placeholder(db: &Db, run_id: &str, parsed: Option<&ParsedTranscript>) -> bool {
    if let Some(parsed) = parsed {
        if apply_synced_title(db, run_id, Some(PLACEHOLDER_TITLE), 1, parsed).await {
            return true;
        }
    }
    let res = sqlx::query("UPDATE runs SET title_auto = 0 WHERE id = ? AND title_auto = 1 AND title = ?")
        .bind(run_id)
        .bind(PLACEHOLDER_TITLE)
        .execute(db)
        .await;
    changed_one(run_id, res);
    false
}

fn changed_one(run_id: &str, res: Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error>) -> bool {
    match res {
        Ok(r) => r.rows_affected() == 1,
        Err(e) => {
            tracing::warn!(event = "session_title_write_failed", run_id, error = %e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::sessions::{parse_transcript, pick_claude_transcript, upsert_session_run_core, SyncOutcome};
    use std::path::{Path, PathBuf};

    /// The lines Claude Code writes for a slash command, tags in its newer order.
    fn slash(cwd: &str, name: &str, args: &str) -> String {
        let text = format!(
            "<command-message>{}</command-message>\n<command-name>{name}</command-name>\n<command-args>{args}</command-args>",
            name.trim_start_matches('/')
        );
        let caveat = serde_json::json!({"type": "user", "isMeta": true, "cwd": cwd, "message": {"role": "user", "content": "<local-command-caveat>Caveat: local commands.</local-command-caveat>"}});
        let command = serde_json::json!({"type": "user", "cwd": cwd, "message": {"role": "user", "content": text}});
        // A skill command expands into a meta message: not the user's words.
        let expanded = serde_json::json!({"type": "user", "isMeta": true, "cwd": cwd, "message": {"role": "user", "content": [{"type": "text", "text": "Base directory for this skill: …"}]}});
        format!("{caveat}\n{command}\n{expanded}\n")
    }

    fn user(cwd: &str, text: &str) -> String {
        serde_json::json!({"type": "user", "cwd": cwd, "message": {"role": "user", "content": text}}).to_string() + "\n"
    }

    fn reply(cwd: &str) -> String {
        serde_json::json!({"type": "assistant", "cwd": cwd, "message": {"role": "assistant", "content": [{"type": "text", "text": "ok"}], "usage": {"input_tokens": 3, "output_tokens": 2}}}).to_string() + "\n"
    }

    fn ai_title(t: &str) -> String {
        serde_json::json!({"type": "ai-title", "aiTitle": t}).to_string() + "\n"
    }

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("puppeteer-session-titles-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("proj").join(".puppeteer").join("runs")).unwrap();
        root
    }

    fn parse(content: &str) -> ParsedTranscript {
        let root = temp_root();
        let file = root.join("s.jsonl");
        std::fs::write(&file, content).unwrap();
        let parsed = parse_transcript(&file, "/proj").expect("a session");
        let _ = std::fs::remove_dir_all(&root);
        parsed
    }

    #[test]
    fn a_slash_command_with_arguments_names_the_session() {
        let newer = "<command-message>us-acceptance</command-message>\n<command-name>/us-acceptance</command-name>\n<command-args>#945</command-args>";
        assert_eq!(slash_command_title(newer).as_deref(), Some("/us-acceptance #945"));
        let older = "<command-name>/us</command-name>\n            <command-message>us</command-message>\n            <command-args>phân tích   comment\n#12</command-args>";
        assert_eq!(slash_command_title(older).as_deref(), Some("/us phân tích comment #12"));
        assert_eq!(slash_command_title("<command-name>us</command-name><command-args>x</command-args>").as_deref(), Some("/us x"));
        for none in [
            "<command-name>/clear</command-name><command-args>anything</command-args>",
            "<command-name>/usage</command-name>\n<command-message>usage</command-message>\n<command-args></command-args>",
            "<command-name>/compact</command-name>",
            "<local-command-stdout>done</local-command-stdout>",
            "<command-name>/us</command-name><command-args>   </command-args>",
        ] {
            assert_eq!(slash_command_title(none), None, "{none}");
        }
        let long = format!("<command-name>/us</command-name><command-args>{}</command-args>", "word ".repeat(80));
        assert!(slash_command_title(&long).unwrap().chars().count() <= TITLE_MAX_CHARS + 1);
    }

    #[test]
    fn the_parse_title_follows_ai_title_then_message_then_slash_command() {
        let cwd = "/proj";
        let p = parse(&(slash(cwd, "/us-acceptance", "#945") + &reply(cwd)));
        assert_eq!((p.title.as_str(), p.ai_title.as_deref()), ("/us-acceptance #945", None));

        let p = parse(&(slash(cwd, "/clear", "") + &slash(cwd, "/us", "phân tích comment") + &reply(cwd)));
        assert_eq!(p.title, "/us phân tích comment");

        let p = parse(&(slash(cwd, "/clear", "") + &slash(cwd, "/usage", "") + &reply(cwd)));
        assert_eq!(p.title, PLACEHOLDER_TITLE);

        let p = parse(&(slash(cwd, "/us", "abc") + &user(cwd, "Fix the login bug") + &reply(cwd)));
        assert_eq!(p.title, "Fix the login bug");

        let p = parse(&(ai_title("First") + &user(cwd, "Fix the login bug") + &reply(cwd) + &ai_title("Auth fix")));
        assert_eq!((p.title.as_str(), p.ai_title.as_deref()), ("Auth fix", Some("Auth fix")));
        // The slash command never shows up in the log as a user turn.
        assert!(!p.log.contains("command-name"), "{}", p.log);
    }

    struct Fixture {
        root: PathBuf,
        cwd: String,
        file: PathBuf,
        db: Db,
    }

    impl Fixture {
        async fn new(transcript: &str) -> Self {
            let root = temp_root();
            let cwd = root.join("proj").to_string_lossy().to_string();
            let file = root.join("s1.jsonl");
            std::fs::write(&file, transcript.replace("/proj", &cwd)).unwrap();
            let db = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
            sqlx::migrate!("./migrations").run(&db).await.unwrap();
            sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'p', ?, 'c')")
                .bind(&cwd)
                .execute(&db)
                .await
                .unwrap();
            Fixture { root, cwd, file, db }
        }

        /// An existing run `r1` for the session; `synced` = how much of the
        /// transcript it mirrored (its full size = the file did not grow).
        async fn run(&self, status: &str, title: &str, title_auto: i64, synced: i64) {
            sqlx::query(
                "INSERT INTO runs (id, project_id, type, status, engine, session_id, transcript_path, transcript_synced_size,
                                   title, title_auto, created_at, last_activity_at)
                 VALUES ('r1', 'p1', 'session', ?, 'claude', 's1', ?, ?, ?, ?, 'c', 'then')",
            )
            .bind(status)
            .bind(self.file.to_string_lossy().as_ref())
            .bind(synced)
            .bind(title)
            .bind(title_auto)
            .execute(&self.db)
            .await
            .unwrap();
        }

        fn size(&self) -> i64 {
            std::fs::metadata(&self.file).unwrap().len() as i64
        }

        async fn sync(&self) -> String {
            let outcome =
                upsert_session_run_core(&self.db, "p1", "p", &self.cwd, "s1", "claude", &self.file, parse_transcript, true, false)
                    .await
                    .unwrap();
            match outcome {
                SyncOutcome::Imported(_) => "imported".into(),
                SyncOutcome::Updated(id) => format!("updated {id}"),
                SyncOutcome::Skipped => "skipped".into(),
            }
        }

        async fn title(&self) -> (String, i64) {
            sqlx::query_as("SELECT title, title_auto FROM runs WHERE session_id = 's1'").fetch_one(&self.db).await.unwrap()
        }

        fn log(&self) -> PathBuf {
            Path::new(&self.cwd).join(".puppeteer").join("runs").join("r1.log")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[tokio::test]
    async fn an_import_is_app_named_unless_the_title_is_an_ai_title() {
        let f = Fixture::new(&(slash("/proj", "/us-acceptance", "#945") + &reply("/proj"))).await;
        assert_eq!(f.sync().await, "imported");
        assert_eq!(f.title().await, ("/us-acceptance #945".into(), 1));

        let f = Fixture::new(&(slash("/proj", "/usage", "") + &reply("/proj"))).await;
        f.sync().await;
        assert_eq!(f.title().await, (PLACEHOLDER_TITLE.into(), 1));

        let f = Fixture::new(&(user("/proj", "Fix it") + &reply("/proj") + &ai_title("Auth fix"))).await;
        f.sync().await;
        assert_eq!(f.title().await, ("Auth fix".into(), 0));
    }

    #[tokio::test]
    async fn a_grown_transcript_brings_its_ai_title_to_an_app_named_run_only() {
        let content = user("/proj", "Fix the login bug") + &reply("/proj") + &ai_title("T");
        let f = Fixture::new(&content).await;
        f.run("done", "Fix the login bug", 1, 1).await;
        assert_eq!(f.sync().await, "updated r1");
        assert_eq!(f.title().await, ("T".into(), 0));

        let f = Fixture::new(&content).await;
        f.run("done", "Mine", 0, 1).await;
        assert_eq!(f.sync().await, "updated r1");
        assert_eq!(f.title().await, ("Mine".into(), 0));
    }

    #[tokio::test]
    async fn an_old_imported_session_takes_its_ai_title_without_touching_the_rest() {
        let f = Fixture::new(&(slash("/proj", "/us", "abc") + &reply("/proj") + &ai_title("T"))).await;
        f.run("done", PLACEHOLDER_TITLE, 1, f.size()).await;
        assert_eq!(f.sync().await, "updated r1");
        assert_eq!(f.title().await, ("T".into(), 0));
        // Title only: no log, no usage, same size marker and activity time.
        assert!(!f.log().exists());
        let (size, activity, usage): (i64, String, i64) = sqlx::query_as(
            "SELECT transcript_synced_size, last_activity_at, (SELECT COUNT(*) FROM run_usage WHERE run_id = 'r1') FROM runs WHERE id = 'r1'",
        )
        .fetch_one(&f.db)
        .await
        .unwrap();
        assert_eq!((size, activity.as_str(), usage), (f.size(), "then", 0));
        // Named: the unchanged transcript is left alone from now on.
        assert_eq!(f.sync().await, "skipped");
    }

    #[tokio::test]
    async fn an_old_imported_session_takes_its_slash_command_and_stays_app_named() {
        let f = Fixture::new(&(slash("/proj", "/clear", "") + &slash("/proj", "/us", "abc") + &reply("/proj"))).await;
        f.run("done", PLACEHOLDER_TITLE, 1, f.size()).await;
        assert_eq!(f.sync().await, "updated r1");
        assert_eq!(f.title().await, ("/us abc".into(), 1));
        assert_eq!(f.sync().await, "skipped");
    }

    #[tokio::test]
    async fn an_old_imported_session_with_nothing_to_name_it_is_read_once() {
        let f = Fixture::new(&(slash("/proj", "/usage", "") + &reply("/proj"))).await;
        f.run("done", PLACEHOLDER_TITLE, 1, f.size()).await;
        assert_eq!(f.sync().await, "skipped");
        assert_eq!(f.title().await, (PLACEHOLDER_TITLE.into(), 0));
        // Not app-named any more: the next sync of the same file skips before parsing.
        assert!(!needs_repair(Some(PLACEHOLDER_TITLE), 0));
        assert_eq!(f.sync().await, "skipped");
        assert_eq!(f.title().await, (PLACEHOLDER_TITLE.into(), 0));
    }

    #[tokio::test]
    async fn a_running_or_vanished_imported_session_is_left_alone() {
        let f = Fixture::new(&(user("/proj", "hi") + &reply("/proj") + &ai_title("T"))).await;
        f.run("running", PLACEHOLDER_TITLE, 1, f.size()).await;
        assert_eq!(f.sync().await, "skipped");
        assert_eq!(f.title().await, (PLACEHOLDER_TITLE.into(), 1));

        // A transcript gone from disk is never picked, so its run is not synced.
        let gone = f.root.join("gone.jsonl");
        assert_eq!(pick_claude_transcript(&[f.root.clone()], "gone", Some(&gone), None), None);
    }
}
