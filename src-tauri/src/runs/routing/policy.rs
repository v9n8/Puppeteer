//! Layer 1: the routing policy a new Claude session gets appended to its system
//! prompt. A policy is a table (task kind → tier and floor) and a version; its
//! text is built from the table by [`policy_text`] and stored whole in
//! `routing_policies`, so a resume reads the stored bytes back instead of
//! building them again (AC-06).
//!
//! The text is English and deterministic: same version and table, same bytes.
//! It names no session model (the ceiling is a rule in words), so a version has
//! one text for every session.

use serde::{Deserialize, Serialize};

use super::tier::{Effort, Model, Tier};
use crate::conductor::defaults::TASK_KINDS;
use crate::db::Db;

/// One task kind's row. `tier: None` = the agent leaves `model` and `effort`
/// unset (the app's fallbacks apply); `floor: None` = no floor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KindRule {
    pub kind: String,
    pub tier: Option<Tier>,
    pub floor: Option<Tier>,
}

/// Version 1, shipped with the app (QO-FR27): conservative, so writing,
/// reviewing and designing stay on Opus; layer 3c lowers tiers from the data.
pub fn v1_table() -> Vec<KindRule> {
    use Effort::*;
    use Model::*;
    let rule = |kind: &str, tier: Option<Tier>, floor: Option<Tier>| KindRule { kind: kind.to_string(), tier, floor };
    vec![
        rule("search", Some(Tier::new(Sonnet, Low)), Some(Tier::new(Haiku, Low))),
        rule("docs", Some(Tier::new(Sonnet, Medium)), Some(Tier::new(Sonnet, Low))),
        rule("test", Some(Tier::new(Sonnet, Medium)), Some(Tier::new(Sonnet, Low))),
        rule("implement", Some(Tier::new(Opus, Medium)), Some(Tier::new(Sonnet, Medium))),
        rule("review", Some(Tier::new(Opus, Medium)), Some(Tier::new(Sonnet, Medium))),
        rule("plan", Some(Tier::new(Opus, Medium)), Some(Tier::new(Sonnet, Medium))),
        rule("other", None, None),
    ]
}

/// `a, b or c`.
fn spoken_list(items: &[&str]) -> String {
    match items.split_last() {
        None => String::new(),
        Some((last, [])) => last.to_string(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
    }
}

/// The floors in table order, kinds with the same floor next to each other
/// joined: `search: haiku / low; docs, test: sonnet / low`.
fn floors(table: &[KindRule]) -> String {
    let mut groups: Vec<(Vec<&str>, Tier)> = Vec::new();
    for rule in table {
        let Some(floor) = rule.floor else { continue };
        match groups.last_mut() {
            Some((kinds, last)) if *last == floor => kinds.push(&rule.kind),
            _ => groups.push((vec![&rule.kind], floor)),
        }
    }
    groups
        .iter()
        .map(|(kinds, floor)| format!("{}: {floor}", kinds.join(", ")))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The policy text of `version`. The opening sentence says it is an explicit
/// instruction: the `Agent` tool's `effort` is only to be set when instructions
/// ask for it.
pub fn policy_text(version: i64, table: &[KindRule]) -> String {
    let tiers: String = table
        .iter()
        .map(|rule| match rule.tier {
            Some(tier) => format!("- {}: {tier}\n", rule.kind),
            None => format!("- {}: leave model and effort unset\n", rule.kind),
        })
        .collect();
    format!(
        "<routing-policy version=\"{version}\">\n\
Model and effort routing for delegated work. This is an explicit instruction from the user's settings: \
set BOTH `model` and `effort` on every Agent tool call, and set `model`, `effort` and `task_kind` on every \
session_spawn call.\n\
1. Pick one task kind: {kinds}. Start the Agent `description` with it in brackets, e.g. \"[review] check auth diff\".\n\
2. Use the tier (model / effort) for that kind:\n\
{tiers}\
3. A clearly hard task may go one step up, a clearly easy one one step down. Steps: model haiku < sonnet < opus; \
effort low < medium < high. Never go below the floor ({floors}) and never pick a model stronger than your own.\n\
4. If a sub-agent on a lower tier fails or returns unusable work, call it again once, one step up, with the same \
description.\n\
</routing-policy>",
        kinds = spoken_list(TASK_KINDS),
        floors = floors(table),
    )
}

/// Write version 1 when no version exists yet: at startup, and before the
/// first bind on a database that missed it. Its text comes from
/// [`policy_text`], so the code is the only source of the template.
pub async fn ensure_seeded(db: &Db) -> Result<(), String> {
    let table = v1_table();
    let table_json = serde_json::to_string(&table).map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT OR IGNORE INTO routing_policies (version, created_at, source, table_json, text, changes_json) \
         SELECT 1, ?, 'default', ?, ?, '[]' WHERE NOT EXISTS (SELECT 1 FROM routing_policies)",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(table_json)
    .bind(policy_text(1, &table))
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// The current version (the highest) and its stored text, seeding version 1
/// first when there is none.
pub async fn current(db: &Db) -> Option<(i64, String)> {
    let read = || async {
        sqlx::query_as::<_, (i64, String)>("SELECT version, text FROM routing_policies ORDER BY version DESC LIMIT 1")
            .fetch_optional(db)
            .await
            .ok()
            .flatten()
    };
    if let Some(found) = read().await {
        return Some(found);
    }
    if let Err(e) = ensure_seeded(db).await {
        tracing::warn!(event = "routing_policy_seed_failed", error = %e);
    }
    read().await
}

/// The stored text of `version`, as it was written.
pub async fn text_of(db: &Db, version: i64) -> Option<String> {
    sqlx::query_scalar("SELECT text FROM routing_policies WHERE version = ?")
        .bind(version)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

/// Setting: whether a new session gets the policy. Unset = on; `"false"` = off.
/// Portable, and a Remote Control controller may set it (audited with
/// `update_setting`): it changes cost, it is no safety limit.
pub const ENABLED_SETTING: &str = "routing.policy_enabled";

pub(crate) async fn enabled(db: &Db) -> bool {
    let value: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(ENABLED_SETTING)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
    value.as_deref().map(str::trim) != Some("false")
}

/// The stored text of a bound version: `N > 0` → its text; `0` (none on
/// purpose) or `NULL` (never bound) → nothing.
async fn text_for_version(db: &Db, version: Option<i64>) -> Option<String> {
    match version {
        Some(v) if v > 0 => text_of(db, v).await,
        _ => None,
    }
}

/// Bind a run to a policy version at its start and return the text to append.
/// Bound once: a run that already has a conversation (`session_id`) or a
/// version keeps what it has, so its system prompt never moves (AC-06).
/// - Codex: `0` (no `Agent` tool with a model to set).
/// - Worker: its conductor's version (`0` when the conductor has none).
/// - Top-level: the current version while [`ENABLED_SETTING`] is on, else `0`.
pub async fn bind_for_start(db: &Db, run_id: &str, engine: &str, is_worker: bool) -> Option<String> {
    let (session_id, bound, conductor): (Option<String>, Option<i64>, Option<String>) =
        sqlx::query_as("SELECT session_id, routing_policy_version, conductor_run_id FROM runs WHERE id = ?")
            .bind(run_id)
            .fetch_optional(db)
            .await
            .ok()
            .flatten()?;
    if session_id.is_some() || bound.is_some() {
        return text_for_version(db, bound).await;
    }
    let version = if engine != "claude" {
        0
    } else if is_worker {
        let conductor_version: Option<i64> = sqlx::query_scalar("SELECT routing_policy_version FROM runs WHERE id = ?")
            .bind(conductor.as_deref())
            .fetch_optional(db)
            .await
            .ok()
            .flatten()
            .flatten();
        conductor_version.filter(|v| *v > 0).unwrap_or(0)
    } else if enabled(db).await {
        current(db).await.map_or(0, |(v, _)| v)
    } else {
        0
    };
    if let Err(e) = sqlx::query("UPDATE runs SET routing_policy_version = ? WHERE id = ?")
        .bind(version)
        .bind(run_id)
        .execute(db)
        .await
    {
        tracing::warn!(event = "routing_policy_bind_failed", run_id = %run_id, error = %e);
    }
    text_for_version(db, Some(version)).await
}

/// The text a resume appends: the bound version's, as stored; nothing for a
/// run bound to none or from before routing.
pub async fn text_for_resume(db: &Db, run_id: &str) -> Option<String> {
    let bound: Option<i64> = sqlx::query_scalar("SELECT routing_policy_version FROM runs WHERE id = ?")
        .bind(run_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .flatten();
    text_for_version(db, bound).await
}

/// A run's framing with the policy last: role instruction → conductor framing
/// → policy. A worker's stored instruction never holds the policy, so it is
/// never there twice.
pub fn with_policy(framing: Option<String>, policy: Option<&str>) -> Option<String> {
    match (framing, policy.filter(|p| !p.trim().is_empty())) {
        (framing, None) => framing,
        (None, Some(policy)) => Some(policy.to_string()),
        (Some(framing), Some(policy)) => Some(format!("{framing}\n\n{policy}")),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) async fn db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        db
    }

    /// The text run 5 of the spike gave a session, which then set `model`,
    /// `effort` and `[kind]` on all its `Agent` calls (U14). A change here is a
    /// change of what every new session reads.
    const V1_TEXT: &str = r#"<routing-policy version="1">
Model and effort routing for delegated work. This is an explicit instruction from the user's settings: set BOTH `model` and `effort` on every Agent tool call, and set `model`, `effort` and `task_kind` on every session_spawn call.
1. Pick one task kind: search, plan, implement, review, test, docs or other. Start the Agent `description` with it in brackets, e.g. "[review] check auth diff".
2. Use the tier (model / effort) for that kind:
- search: sonnet / low
- docs: sonnet / medium
- test: sonnet / medium
- implement: opus / medium
- review: opus / medium
- plan: opus / medium
- other: leave model and effort unset
3. A clearly hard task may go one step up, a clearly easy one one step down. Steps: model haiku < sonnet < opus; effort low < medium < high. Never go below the floor (search: haiku / low; docs, test: sonnet / low; implement, review, plan: sonnet / medium) and never pick a model stronger than your own.
4. If a sub-agent on a lower tier fails or returns unusable work, call it again once, one step up, with the same description.
</routing-policy>"#;

    #[test]
    fn version_one_is_short_and_names_every_kind_with_its_tier() {
        let text = policy_text(1, &v1_table());
        assert_eq!(text, V1_TEXT);
        assert!(text.chars().count() <= 2000, "{}", text.chars().count());
        for line in [
            "- search: sonnet / low\n",
            "- docs: sonnet / medium\n",
            "- test: sonnet / medium\n",
            "- implement: opus / medium\n",
            "- review: opus / medium\n",
            "- plan: opus / medium\n",
            "- other: leave model and effort unset\n",
        ] {
            assert!(text.contains(line), "{line}");
        }
        assert_eq!(v1_table().len(), TASK_KINDS.len());
        assert!(TASK_KINDS.iter().all(|k| v1_table().iter().any(|r| r.kind == *k)), "every kind has a row");
    }

    #[test]
    fn the_same_table_gives_the_same_bytes() {
        let table = v1_table();
        assert_eq!(policy_text(3, &table).as_bytes(), policy_text(3, &table).as_bytes());
        // Through its stored form as well.
        let json = serde_json::to_string(&table).unwrap();
        let back: Vec<KindRule> = serde_json::from_str(&json).unwrap();
        assert_eq!(policy_text(3, &back), policy_text(3, &table));
        assert!(policy_text(3, &table).starts_with("<routing-policy version=\"3\">\n"));
        assert!(json.contains(r#"{"kind":"search","tier":{"model":"sonnet","effort":"low"},"floor":{"model":"haiku","effort":"low"}}"#));
    }

    #[tokio::test]
    async fn version_one_is_seeded_once_from_the_code() {
        let db = db().await;
        assert_eq!(text_of(&db, 1).await, None);
        ensure_seeded(&db).await.unwrap();
        ensure_seeded(&db).await.unwrap();
        let rows: Vec<(i64, String, String)> =
            sqlx::query_as("SELECT version, source, changes_json FROM routing_policies").fetch_all(&db).await.unwrap();
        assert_eq!(rows, vec![(1, "default".into(), "[]".into())]);
        assert_eq!(text_of(&db, 1).await.as_deref(), Some(V1_TEXT));
        assert_eq!(current(&db).await, Some((1, V1_TEXT.to_string())));
    }

    #[tokio::test]
    async fn current_seeds_a_database_that_missed_it_and_follows_the_highest_version() {
        let db = db().await;
        assert_eq!(current(&db).await.map(|(v, _)| v), Some(1));
        sqlx::query("INSERT INTO routing_policies (version, created_at, source, table_json, text) VALUES (2, 'c', 'auto', '[]', 'V2')")
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(current(&db).await, Some((2, "V2".to_string())));
        // A later seed never adds version 1 again.
        ensure_seeded(&db).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM routing_policies").fetch_one(&db).await.unwrap();
        assert_eq!(count, 2);
    }

    async fn run(db: &Db, id: &str, engine: &str, role: Option<&str>, conductor: Option<&str>) {
        sqlx::query("INSERT INTO runs (id, project_id, type, engine, role, conductor_run_id, created_at) VALUES (?, 'p1', 'session', ?, ?, ?, 'c')")
            .bind(id)
            .bind(engine)
            .bind(role)
            .bind(conductor)
            .execute(db)
            .await
            .unwrap();
    }

    async fn bound(db: &Db, id: &str) -> Option<i64> {
        sqlx::query_scalar("SELECT routing_policy_version FROM runs WHERE id = ?").bind(id).fetch_one(db).await.unwrap()
    }

    async fn add_version(db: &Db, version: i64, text: &str) {
        sqlx::query("INSERT INTO routing_policies (version, created_at, source, table_json, text) VALUES (?, 'c', 'auto', '[]', ?)")
            .bind(version)
            .bind(text)
            .execute(db)
            .await
            .unwrap();
    }

    async fn policy_db() -> Db {
        let db = crate::commands::response_styles::tests::test_db().await;
        ensure_seeded(&db).await.unwrap();
        db
    }

    #[tokio::test]
    async fn a_new_claude_session_gets_the_current_version_and_keeps_it() {
        let db = policy_db().await;
        add_version(&db, 2, "POLICY V2").await;
        run(&db, "r1", "claude", Some("conductor"), None).await;
        assert_eq!(bind_for_start(&db, "r1", "claude", false).await.as_deref(), Some("POLICY V2"));
        assert_eq!(bound(&db, "r1").await, Some(2));
        // A newer version later: the run keeps its own, on a start and a resume.
        add_version(&db, 3, "POLICY V3").await;
        assert_eq!(bind_for_start(&db, "r1", "claude", false).await.as_deref(), Some("POLICY V2"));
        assert_eq!(text_for_resume(&db, "r1").await.as_deref(), Some("POLICY V2"));
        assert_eq!(bound(&db, "r1").await, Some(2));
    }

    /// AC-06 / QO-AC33: what the session's first turn appended is what every
    /// resume sends, byte for byte, after the policy moved on; a resume that
    /// has to start a new session gets the bound version again, not the new one.
    #[tokio::test]
    async fn a_resume_sends_the_bound_policy_byte_for_byte() {
        use crate::runs::claude_prompt::{compose_and_record, resume_env, APPEND_ENV};
        let db = policy_db().await;
        run(&db, "r1", "claude", Some("conductor"), None).await;
        let framing = || Some("FRAMING".to_string());
        let policy = bind_for_start(&db, "r1", "claude", false).await;
        let first = compose_and_record(&db, "r1", None, with_policy(framing(), policy.as_deref()).as_deref(), None)
            .await
            .unwrap();
        assert_eq!(first, format!("FRAMING\n\n{V1_TEXT}"));
        sqlx::query("UPDATE runs SET session_id = 's1' WHERE id = 'r1'").execute(&db).await.unwrap();
        add_version(&db, 2, "POLICY V2").await;

        let resumed = with_policy(framing(), text_for_resume(&db, "r1").await.as_deref());
        let env = resume_env(&db, "r1", "p1", true, resumed.as_deref(), None, None).await;
        assert_eq!(env, vec![(APPEND_ENV, first.clone())]);
        let fresh = resume_env(&db, "r1", "p1", false, resumed.as_deref(), None, None).await;
        assert_eq!(fresh, vec![(APPEND_ENV, first)]);
        assert_eq!(bound(&db, "r1").await, Some(1));
    }

    #[tokio::test]
    async fn a_session_from_before_routing_resumes_without_a_policy() {
        let db = policy_db().await;
        run(&db, "old", "claude", Some("conductor"), None).await;
        sqlx::query("UPDATE runs SET session_id = 's-old' WHERE id = 'old'").execute(&db).await.unwrap();
        assert_eq!(text_for_resume(&db, "old").await, None);
        assert_eq!(bind_for_start(&db, "old", "claude", false).await, None);
        assert_eq!(bound(&db, "old").await, None, "still never bound");
        assert_eq!(with_policy(Some("FRAMING".into()), None).as_deref(), Some("FRAMING"));
    }

    #[tokio::test]
    async fn switched_off_or_codex_gets_none_and_records_zero() {
        let db = policy_db().await;
        crate::commands::settings::set_setting(&db, ENABLED_SETTING, "false").await.unwrap();
        run(&db, "off", "claude", Some("conductor"), None).await;
        assert_eq!(bind_for_start(&db, "off", "claude", false).await, None);
        assert_eq!(bound(&db, "off").await, Some(0));
        assert_eq!(text_for_resume(&db, "off").await, None);
        // Codex, with the switch on again.
        crate::commands::settings::set_setting(&db, ENABLED_SETTING, "true").await.unwrap();
        run(&db, "cx", "codex", Some("conductor"), None).await;
        assert_eq!(bind_for_start(&db, "cx", "codex", false).await, None);
        assert_eq!(bound(&db, "cx").await, Some(0));
    }

    #[tokio::test]
    async fn a_worker_takes_its_conductors_version() {
        let db = policy_db().await;
        run(&db, "c", "claude", Some("conductor"), None).await;
        bind_for_start(&db, "c", "claude", false).await;
        add_version(&db, 2, "POLICY V2").await;
        run(&db, "w", "claude", Some("worker"), Some("c")).await;
        assert_eq!(bind_for_start(&db, "w", "claude", true).await.as_deref(), Some(V1_TEXT));
        assert_eq!(bound(&db, "w").await, Some(1));
        // Even with the switch off since: the conductor's choice holds.
        crate::commands::settings::set_setting(&db, ENABLED_SETTING, "false").await.unwrap();
        run(&db, "w2", "claude", Some("worker"), Some("c")).await;
        assert_eq!(bind_for_start(&db, "w2", "claude", true).await.as_deref(), Some(V1_TEXT));
        // A conductor with none (Codex, or from before routing): its workers get none.
        run(&db, "cx", "codex", Some("conductor"), None).await;
        bind_for_start(&db, "cx", "codex", false).await;
        run(&db, "w3", "claude", Some("worker"), Some("cx")).await;
        assert_eq!(bind_for_start(&db, "w3", "claude", true).await, None);
        assert_eq!(bound(&db, "w3").await, Some(0));
        run(&db, "w4", "claude", Some("worker"), Some("gone")).await;
        assert_eq!(bind_for_start(&db, "w4", "claude", true).await, None);
        assert_eq!(bound(&db, "w4").await, Some(0));
        // A Codex worker of a conductor with a policy: none.
        run(&db, "w5", "codex", Some("worker"), Some("c")).await;
        assert_eq!(bind_for_start(&db, "w5", "codex", true).await, None);
    }

    #[test]
    fn the_policy_goes_last_in_the_framing() {
        assert_eq!(with_policy(None, None), None);
        assert_eq!(with_policy(None, Some("P")).as_deref(), Some("P"));
        assert_eq!(with_policy(Some("F".into()), Some("P")).as_deref(), Some("F\n\nP"));
        assert_eq!(with_policy(Some("F".into()), Some(" ")).as_deref(), Some("F"));
    }

    #[test]
    fn the_switch_is_portable_and_remote_may_set_it() {
        assert!(crate::commands::settings::is_portable_setting(ENABLED_SETTING));
        assert!(crate::remote::guard::remote_may_set_setting(ENABLED_SETTING));
    }

    fn section<'a>(src: &'a str, from: &str, to: &str) -> &'a str {
        let body = &src[src.find(from).unwrap()..];
        &body[..body.find(to).unwrap()]
    }

    /// The start needs an AppHandle, so the source is checked: the start binds
    /// once and wraps its framing, and the resume framing (both engines) adds
    /// the bound text.
    #[test]
    fn start_binds_and_resume_reads_the_bound_text() {
        let runs = include_str!("../../commands/runs.rs").replace("\r\n", "\n");
        let start = section(&runs, "pub(crate) async fn start_run_inner(", "#[tauri::command]");
        assert_eq!(start.matches("routing::policy::bind_for_start(db, &payload.run_id, &engine, is_worker)").count(), 1);
        assert!(start.contains("let extra_system_prompt: Option<String> = crate::runs::routing::policy::with_policy("));
        let resume = section(&runs, "pub(crate) async fn resume_run_with(", "\n}\n");
        assert!(!resume.contains("bind_for_start"), "a resume never binds");
        let conductor = include_str!("../../conductor/commands.rs").replace("\r\n", "\n");
        let framing = section(&conductor, "pub(crate) async fn resume_framing(", "\n}\n");
        assert!(framing.contains("routing::policy::with_policy("));
        assert!(framing.contains("routing::policy::text_for_resume(db, run_id).await"));
    }
}
