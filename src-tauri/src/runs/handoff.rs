//! AC-19: a leaner cross-engine handoff (`create_handoff_run`).
//!
//! The new run used to be told to read the whole prior transcript, so its
//! first turn loaded tens of thousands of tokens and still had to work out
//! where the work stood. The context file now starts with a short handoff
//! summary written by the side-task engine (`translate.rs`: tool-less, `haiku`
//! by default) and keeps the full transcript below it, to read only when a
//! detail is missing. A failed or slow summary never blocks the handoff: the
//! file is then written exactly as before, without one.
//!
//! Experiment (off unless [`CODEX_IMPORT_FLAG_ENV`] is `1` in Puppeteer's own
//! env): a Claude → Codex handoff also asks Codex to import the Claude session
//! JSONL itself (`externalAgentConfig/import`, like codex-plugin-cc), so the
//! Codex thread starts with the conversation as history. The sidecar falls
//! back to a fresh thread when the import does not work, and the context file
//! is written either way.

use crate::commands::translate::{self, TranslateState, TurnEvent};
use crate::db::Db;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;

/// The handoff waits this long for the summary, then goes on without it.
pub const SUMMARY_TIMEOUT: Duration = Duration::from_secs(90);

/// The part of the transcript the summary model is given. A long session can
/// exceed the small model's window, so the middle is cut: the start holds the
/// goal, the end holds where the work stands.
const SUMMARY_INPUT_HEAD_CHARS: usize = 40_000;
const SUMMARY_INPUT_TAIL_CHARS: usize = 160_000;

/// Headings of the context file. The new run's prompt (`run.handoffSeedSummary`)
/// names the summary by its line range, so these only need to read well.
const SUMMARY_HEADING: &str = "## Tóm tắt bàn giao";
const DETAIL_HEADING: &str = "## Chi tiết, chỉ đọc khi cần";

/// Fixed in code (not a setting) because the new run's prompt relies on what
/// it produces: a short brief with these sections, in the conversation's
/// language, with no preamble. "Do not invent" because the brief is read as
/// fact by an agent that will not re-check it against the transcript.
const SUMMARY_PROMPT: &str = "You are writing a handoff brief. Another AI coding agent will continue \
the work below without having seen it, and will read only your brief unless it needs a detail.

Write at most about 1,500 tokens, in the main language of the conversation, as Markdown with these \
sections (skip one only when there is nothing for it):
- Goal: what the user wants in the end.
- Done so far: what has been done and verified.
- Files changed: paths, one line each on what changed.
- Not finished: what is in progress or broken, and why.
- Next step: the very next thing to do, concretely.
- Key decisions: choices made and constraints the user set, with the reason.

Rules: keep exact file paths, commands, names and error messages. Do not invent anything that is \
not in the conversation; say \"unknown\" instead. Output only the brief, with no preamble, and do \
not start it with a top-level heading.

The conversation, between <transcript> tags:";

/// The side-task engine of the summary (its own warm instance, so a handoff
/// never queues behind translations, and a fresh session per summary) and the
/// Claude sessions Codex is asked to import at the start of a handoff run.
#[derive(Clone)]
pub struct HandoffState {
    summary: TranslateState,
    imports: Arc<Mutex<HashMap<String, PathBuf>>>,
}

impl Default for HandoffState {
    fn default() -> Self {
        HandoffState {
            summary: TranslateState::default()
                .with_recycle_after(1)
                .silent()
                .tracked_as(crate::procs::ProcKind::HandoffSummary),
            imports: Arc::default(),
        }
    }
}

impl HandoffState {
    /// Stop the warm summary session (app exit).
    pub async fn shutdown(&self) {
        self.summary.shutdown().await;
    }

    /// Remember the Claude session JSONL the Codex run `run_id` should import.
    pub fn remember_import(&self, run_id: &str, source: PathBuf) {
        if let Ok(mut m) = self.imports.lock() {
            m.insert(run_id.to_string(), source);
        }
    }

    /// The session `run_id` should import, once: a later start of the same run
    /// resumes its own thread instead.
    pub fn take_import(&self, run_id: &str) -> Option<PathBuf> {
        self.imports.lock().ok()?.remove(run_id)
    }
}

/// [`CODEX_IMPORT_FLAG_ENV`] = `1` turns the import experiment on.
pub const CODEX_IMPORT_FLAG_ENV: &str = "PUPPETEER_EXPERIMENTAL_CODEX_IMPORT";
/// How the Codex sidecar gets the Claude session JSONL to import.
pub const CODEX_IMPORT_SESSION_ENV: &str = "PUPPETEER_CODEX_IMPORT_SESSION";

pub fn codex_import_enabled() -> bool {
    std::env::var(CODEX_IMPORT_FLAG_ENV).is_ok_and(|v| v.trim() == "1")
}

/// The Claude session JSONL Codex can import, if there is one. Codex only
/// takes a session under `$HOME/.claude/projects` (codex-cli 0.133
/// `external_agent_session_source_path`), so a session kept in a managed
/// account's config dir is not offered.
pub fn claude_import_source(home: &Path, cwd: &str, session_id: &str) -> Option<PathBuf> {
    if session_id.is_empty() || session_id.contains(['/', '\\']) {
        return None;
    }
    let path = home
        .join(".claude")
        .join("projects")
        .join(crate::commands::sessions::encode_project_dir(cwd))
        .join(format!("{session_id}.jsonl"));
    path.is_file().then_some(path)
}

/// The transcript as the summary model gets it: whole when short, otherwise
/// its start and end around a marker saying how much was left out.
fn clip_for_summary(transcript: &str) -> String {
    let total = transcript.chars().count();
    if total <= SUMMARY_INPUT_HEAD_CHARS + SUMMARY_INPUT_TAIL_CHARS {
        return transcript.to_string();
    }
    let head: String = transcript.chars().take(SUMMARY_INPUT_HEAD_CHARS).collect();
    let tail: String = transcript.chars().skip(total - SUMMARY_INPUT_TAIL_CHARS).collect();
    let cut = total - SUMMARY_INPUT_HEAD_CHARS - SUMMARY_INPUT_TAIL_CHARS;
    format!("{head}\n\n[… {cut} characters of the middle of the conversation left out …]\n\n{tail}")
}

fn summary_prompt(transcript: &str) -> String {
    format!("{SUMMARY_PROMPT}\n\n<transcript>\n{}\n</transcript>", clip_for_summary(transcript))
}

/// Ask the side-task engine for the handoff summary; `Err` on a failure, an
/// empty answer or after [`SUMMARY_TIMEOUT`].
pub async fn summarize(app: &AppHandle, db: &Db, state: &HandoffState, transcript: &str) -> Result<String, String> {
    if transcript.trim().is_empty() {
        return Err("nothing to summarize".into());
    }
    let (turn, rx) = translate::submit_prompt(app, db, &state.summary, summary_prompt(transcript), true).await?;
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
    let text = match tokio::time::timeout(SUMMARY_TIMEOUT, wait).await {
        Ok(r) => r?,
        Err(_) => {
            state.summary.abandon_turn(turn).await;
            return Err(format!("timed out after {}s", SUMMARY_TIMEOUT.as_secs()));
        }
    };
    let text = text.trim();
    if text.is_empty() {
        return Err("empty summary".into());
    }
    Ok(text.to_string())
}

/// The context file of a handoff run, and how many of its first lines hold
/// the summary (`None` without one: the file is then the old, transcript-only
/// one).
pub fn context_file(source_engine: &str, summary: Option<&str>, transcript: &str) -> (String, Option<usize>) {
    let Some(summary) = summary else {
        let body = format!(
            "# Ngữ cảnh phiên trước (engine: {source_engine})\n\nĐây là toàn bộ hội thoại/diễn tiến của phiên làm việc trước. \
Đọc kỹ để nắm những gì đã làm, rồi tiếp tục công việc đang dở.\n\n---\n\n{transcript}\n"
        );
        return (body, None);
    };
    let head = format!(
        "# Ngữ cảnh phiên trước (engine: {source_engine})\n\n{SUMMARY_HEADING}\n\n{}\n\n---\n\n",
        summary.trim()
    );
    // Up to the `---` that closes the summary.
    let summary_lines = head.trim_end().lines().count();
    let body = format!(
        "{head}{DETAIL_HEADING}\n\nToàn bộ hội thoại/diễn tiến của phiên trước, nguyên văn. \
Chỉ đọc phần này khi phần tóm tắt ở trên thiếu chi tiết bạn cần.\n\n{transcript}\n"
    );
    (body, Some(summary_lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_summary_comes_first_and_the_transcript_after_it() {
        let (body, lines) = context_file("claude", Some("- Goal: ship AC-19\n- Next step: run the tests"), "USER: hi\nAI: hello");
        let lines = lines.expect("summary lines");
        let summary_at = body.find(SUMMARY_HEADING).unwrap();
        let goal_at = body.find("- Goal: ship AC-19").unwrap();
        let detail_at = body.find(DETAIL_HEADING).unwrap();
        let transcript_at = body.find("USER: hi\nAI: hello").unwrap();
        assert!(summary_at < goal_at && goal_at < detail_at && detail_at < transcript_at);
        // The line range the prompt names holds the whole summary and stops
        // right before the details.
        let all: Vec<&str> = body.lines().collect();
        assert_eq!(all[lines - 1], "---");
        assert_eq!(all[lines + 1], DETAIL_HEADING);
        assert!(all[..lines].contains(&"- Next step: run the tests"));
        assert!(!all[..lines].iter().any(|l| l.contains("USER: hi")));
    }

    #[test]
    fn without_a_summary_the_file_is_the_old_one() {
        let (body, lines) = context_file("codex", None, "USER: hi");
        assert_eq!(lines, None);
        assert_eq!(
            body,
            "# Ngữ cảnh phiên trước (engine: codex)\n\nĐây là toàn bộ hội thoại/diễn tiến của phiên làm việc trước. \
Đọc kỹ để nắm những gì đã làm, rồi tiếp tục công việc đang dở.\n\n---\n\nUSER: hi\n"
        );
        assert!(!body.contains(SUMMARY_HEADING) && !body.contains(DETAIL_HEADING));
    }

    #[test]
    fn a_long_transcript_keeps_its_start_and_end_for_the_summary() {
        let short = "a".repeat(1_000);
        assert_eq!(clip_for_summary(&short), short);

        let long = format!(
            "GOAL{}{}END",
            "x".repeat(SUMMARY_INPUT_HEAD_CHARS),
            "y".repeat(SUMMARY_INPUT_TAIL_CHARS)
        );
        let clipped = clip_for_summary(&long);
        assert!(clipped.starts_with("GOAL") && clipped.ends_with("END"));
        assert!(clipped.contains("[… 7 characters of the middle of the conversation left out …]"));
        assert!(clipped.chars().count() < long.chars().count() + 100);

        let prompt = summary_prompt("USER: fix the bug");
        assert!(prompt.starts_with(SUMMARY_PROMPT));
        assert!(prompt.ends_with("<transcript>\nUSER: fix the bug\n</transcript>"));
    }

    #[test]
    fn only_a_session_under_the_home_claude_store_is_offered_for_import() {
        let home = std::env::temp_dir().join(format!("puppeteer-handoff-{}", uuid::Uuid::new_v4()));
        let dir = home.join(".claude").join("projects").join("-repo-x");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sess-1.jsonl"), "{}\n").unwrap();

        assert_eq!(claude_import_source(&home, "/repo/x", "sess-1"), Some(dir.join("sess-1.jsonl")));
        assert_eq!(claude_import_source(&home, "/repo/x", "sess-2"), None);
        assert_eq!(claude_import_source(&home, "/repo/y", "sess-1"), None);
        assert_eq!(claude_import_source(&home, "/repo/x", ""), None);
        assert_eq!(claude_import_source(&home, "/repo/x", "../sess-1"), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn an_import_is_handed_to_its_run_once() {
        let state = HandoffState::default();
        state.remember_import("run-1", PathBuf::from("/s.jsonl"));
        assert_eq!(state.take_import("run-2"), None);
        assert_eq!(state.take_import("run-1"), Some(PathBuf::from("/s.jsonl")));
        assert_eq!(state.take_import("run-1"), None);
    }
}
