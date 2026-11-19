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
}
