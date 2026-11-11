//! What Claude's system prompt is appended with (git-account context, framing,
//! response style), kept the same on every turn of a session (AC-06).
//!
//! Each turn is a new sidecar that resumes the session, so the append is sent
//! again every time. The prompt cache is a prefix (tools → system → messages):
//! a system prompt that moved by one byte misses the cache for the whole
//! conversation. And where the SDK records the first turn's prompt
//! (`systemPrompt.snapshot`), a different append on a later turn is ignored
//! anyway. So the first turn's append is recorded (`runs.claude_append`) and
//! every resume of that session sends it again as it is. A style picked since
//! then reaches the model as a block before the next message instead
//! ([`response_styles::style_change_seed`]), as on Codex.

use crate::commands::response_styles::{self, ResolvedStyle};
use crate::db::Db;

pub const APPEND_ENV: &str = "PUPPETEER_APPEND_SYSTEM_PROMPT";
pub const SEED_STYLE_ENV: &str = "PUPPETEER_SEED_STYLE";

/// The append of a new session's first turn, recorded for its resumes. `None`:
/// nothing to append.
pub async fn compose_and_record(
    db: &Db,
    run_id: &str,
    account_context: Option<&str>,
    framing: Option<&str>,
    style_block: Option<&str>,
) -> Option<String> {
    let append = response_styles::compose_append(&[account_context, framing, style_block]);
    // '' records "nothing to append", so a resume does not take it for a run
    // from before the column and rebuild it.
    let _ = sqlx::query("UPDATE runs SET claude_append = ? WHERE id = ?")
        .bind(append.as_deref().unwrap_or(""))
        .bind(run_id)
        .execute(db)
        .await;
    append
}

/// The sidecar env of a Claude resume for its system prompt: the session's
/// recorded append ([`APPEND_ENV`]) and, when the style changed since the last
/// turn, the block to put before the next message ([`SEED_STYLE_ENV`]).
/// `resume_session` is false when the resume starts a new session (transcript
/// gone, rewound to the start, a fork that cannot follow): that one gets a new
/// append, with the current style in it.
pub async fn resume_env(
    db: &Db,
    run_id: &str,
    project_id: &str,
    resume_session: bool,
    framing: Option<&str>,
    previous_style: Option<&str>,
    style: Option<&ResolvedStyle>,
) -> Vec<(&'static str, String)> {
    let account_context =
        crate::runs::broker::token::build_account_context(db, project_id, Some(run_id)).await;
    let append = if !resume_session {
        let block = response_styles::resolved_block(style);
        compose_and_record(db, run_id, account_context.as_deref(), framing, block.as_deref()).await
    } else {
        let recorded: Option<String> = sqlx::query_scalar("SELECT claude_append FROM runs WHERE id = ?")
            .bind(run_id)
            .fetch_optional(db)
            .await
            .ok()
            .flatten()
            .flatten();
        match recorded {
            Some(recorded) => {
                // Accepted (the session keeps what it started with), but worth a
                // line when someone wonders why the agent names the old account.
                let changed = [account_context.as_deref(), framing]
                    .into_iter()
                    .flatten()
                    .map(str::trim)
                    .any(|part| !part.is_empty() && !recorded.contains(part));
                if changed {
                    tracing::info!(
                        event = "claude_append_kept",
                        run_id = %run_id,
                        "account context or framing changed since the session started; resuming with the recorded system prompt"
                    );
                }
                Some(recorded).filter(|a| !a.is_empty())
            }
            // A session from before the column: rebuilt with the style its turns
            // had so far (a new one is seeded below), then recorded.
            None => {
                let previous = match previous_style {
                    Some(key) => response_styles::lookup(db, project_id, key).await,
                    None => None,
                };
                let block = response_styles::resolved_block(previous.as_ref());
                compose_and_record(db, run_id, account_context.as_deref(), framing, block.as_deref()).await
            }
        }
    };
    let mut env = Vec::new();
    if let Some(append) = append {
        env.push((APPEND_ENV, append));
    }
    if resume_session {
        if let Some(seed) = response_styles::style_change_seed(previous_style, style) {
            env.push((SEED_STYLE_ENV, seed));
        }
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn db() -> Db {
        let db = crate::commands::response_styles::tests::test_db().await;
        sqlx::query("INSERT INTO runs (id, project_id, type, engine, created_at) VALUES ('r1', 'p1', 'session', 'claude', 'c')")
            .execute(&db)
            .await
            .unwrap();
        db
    }

    async fn recorded(db: &Db) -> Option<String> {
        sqlx::query_scalar("SELECT claude_append FROM runs WHERE id = 'r1'").fetch_one(db).await.unwrap()
    }

    async fn set_recorded(db: &Db, append: Option<&str>) {
        sqlx::query("UPDATE runs SET claude_append = ? WHERE id = 'r1'").bind(append).execute(db).await.unwrap();
    }

    fn style(key: &str, body: Option<&str>) -> ResolvedStyle {
        ResolvedStyle { key: key.into(), body: body.map(str::to_string) }
    }

    fn get<'a>(env: &'a [(&'static str, String)], key: &str) -> Option<&'a str> {
        env.iter().find(|(k, _)| *k == key).map(|(_, v)| v.as_str())
    }

    #[tokio::test]
    async fn the_first_turn_records_its_append() {
        let db = db().await;
        let block = response_styles::style_block("Be short.");
        let append = compose_and_record(&db, "r1", Some("ACCOUNT"), Some("FRAMING"), Some(&block)).await;
        assert_eq!(append.as_deref(), Some(format!("ACCOUNT\n\nFRAMING\n\n{block}").as_str()));
        assert_eq!(recorded(&db).await, append);
        // Nothing to append is recorded too, as ''.
        assert_eq!(compose_and_record(&db, "r1", None, Some(" "), None).await, None);
        assert_eq!(recorded(&db).await.as_deref(), Some(""));
    }

    #[tokio::test]
    async fn every_resume_sends_the_recorded_append_as_it_is() {
        let db = db().await;
        set_recorded(&db, Some("ACCOUNT\n\nFRAMING")).await;
        let concise = style("concise", Some("Be short."));
        // Other framing now, same style: the recorded text, no seed.
        for _ in 0..2 {
            let env = resume_env(&db, "r1", "p1", true, Some("OTHER"), Some("concise"), Some(&concise)).await;
            assert_eq!(env, vec![(APPEND_ENV, "ACCOUNT\n\nFRAMING".to_string())]);
        }
        assert_eq!(recorded(&db).await.as_deref(), Some("ACCOUNT\n\nFRAMING"));
        // Recorded as nothing to append: still nothing, not a rebuild.
        set_recorded(&db, Some("")).await;
        assert!(resume_env(&db, "r1", "p1", true, Some("FRAMING"), None, None).await.is_empty());
        assert_eq!(recorded(&db).await.as_deref(), Some(""));
    }

    #[tokio::test]
    async fn a_changed_style_is_seeded_and_the_append_stays() {
        let db = db().await;
        let block = response_styles::style_block("Be short.");
        set_recorded(&db, Some(&format!("FRAMING\n\n{block}"))).await;
        let review = style("code_review", Some("Review it."));
        let env = resume_env(&db, "r1", "p1", true, Some("FRAMING"), Some("concise"), Some(&review)).await;
        assert_eq!(get(&env, APPEND_ENV), Some(format!("FRAMING\n\n{block}").as_str()));
        let seed = get(&env, SEED_STYLE_ENV).unwrap();
        assert!(seed.starts_with("<session-framing>") && seed.contains("Review it."));
        assert_eq!(Some(seed), response_styles::style_change_seed(Some("concise"), Some(&review)).as_deref());
    }

    #[tokio::test]
    async fn a_run_from_before_the_column_is_rebuilt_with_its_old_style_and_recorded() {
        let db = db().await;
        assert_eq!(recorded(&db).await, None);
        let review = style("code_review", Some("Review it."));
        let env = resume_env(&db, "r1", "p1", true, Some("FRAMING"), Some("concise"), Some(&review)).await;
        let concise_block = response_styles::style_block(response_styles::builtin("concise").unwrap().body);
        let rebuilt = format!("FRAMING\n\n{concise_block}");
        assert_eq!(get(&env, APPEND_ENV), Some(rebuilt.as_str()));
        assert!(get(&env, SEED_STYLE_ENV).unwrap().contains("Review it."));
        assert_eq!(recorded(&db).await.as_deref(), Some(rebuilt.as_str()));
    }

    #[tokio::test]
    async fn a_new_session_on_resume_gets_a_new_append_with_the_current_style() {
        let db = db().await;
        set_recorded(&db, Some("OLD")).await;
        let review = style("code_review", Some("Review it."));
        let env = resume_env(&db, "r1", "p1", false, Some("FRAMING"), Some("concise"), Some(&review)).await;
        let expected = format!("FRAMING\n\n{}", response_styles::style_block("Review it."));
        assert_eq!(env, vec![(APPEND_ENV, expected.clone())]);
        assert_eq!(recorded(&db).await.as_deref(), Some(expected.as_str()));
    }
}
