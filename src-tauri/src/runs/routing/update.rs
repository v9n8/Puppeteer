//! Layer 3c: the routing policy follows the recorded results (QO-FR44 …
//! QO-FR50). No model is called: [`recompute`] moves each task kind at most
//! one step on the 9-tier ladder (`haiku/low` < … < `opus/high`), down when the
//! tier below did well enough and is not below the kind's floor, up when the
//! current tier failed too often (both → up: errors fall on the side of
//! quality). A changed table becomes a new version for new sessions (switch
//! [`AUTO_SETTING`] on) or a proposal shown in Settings › Routing (off).
//!
//! When: a background task checks 60 s after startup, then every 6 hours, and
//! recomputes only when the last time was ≥ 24 h ago and ≥ 20 counted results
//! came in since ([`should_recompute`]); "Update now" skips both gates.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use super::policy::{self, KindRule};
use super::report::{self, DecisionRow, Outcome};
use super::tier::{Effort, Model, Tier};
use crate::db::Db;

/// Setting: whether a changed table becomes a version at once. Unset = on;
/// `"false"` = off (a proposal instead). Portable; Remote may set it.
pub const AUTO_SETTING: &str = "routing.auto_update";
/// Bookkeeping, not portable: when the table was last recomputed (RFC 3339).
pub const LAST_SETTING: &str = "routing.last_recompute_at";
/// Bookkeeping: the changes a recompute found while [`AUTO_SETTING`] was off (JSON).
pub const PROPOSAL_SETTING: &str = "routing.proposal";
/// Bookkeeping: an automatic version the user has not been told about yet.
pub const NOTICE_SETTING: &str = "routing.pending_notice";

/// The keys only this module writes: never portable, never set remotely.
pub fn is_bookkeeping(key: &str) -> bool {
    [LAST_SETTING, PROPOSAL_SETTING, NOTICE_SETTING].contains(&key)
}
/// Emitted with `{ version }` when a recompute made a new version.
pub const UPDATED_EVENT: &str = "routing:policy_updated";

/// A step down needs this many counted results on the tier below …
pub const DOWN_MIN_SAMPLES: u32 = 20;
/// … with at least this share of successes (percent).
pub const DOWN_MIN_SUCCESS_PCT: u32 = 90;
/// A step up needs this many counted results on the current tier …
pub const UP_MIN_SAMPLES: u32 = 5;
/// … with at least this share of failures (percent).
pub const UP_MIN_FAILURE_PCT: u32 = 30;
/// Results older than this are not read, for either direction (QĐ-Q6-4).
pub const WINDOW_DAYS: i64 = 30;
/// A recompute not forced waits this long after the last one …
pub const MIN_HOURS_BETWEEN: i64 = 24;
/// … and for this many counted results finished since.
pub const MIN_NEW_RESULTS: usize = 20;

const MODELS: [Model; 3] = [Model::Haiku, Model::Sonnet, Model::Opus];

fn rung(t: Tier) -> usize {
    MODELS.iter().position(|m| *m == t.model).unwrap_or(0) * 3 + Effort::ALL.iter().position(|e| *e == t.effort).unwrap_or(0)
}

fn at_rung(i: usize) -> Option<Tier> {
    Some(Tier::new(*MODELS.get(i / 3)?, *Effort::ALL.get(i % 3)?))
}

/// The tier one step down the ladder (`sonnet/low` → `haiku/high`).
pub fn step_down(t: Tier) -> Option<Tier> {
    rung(t).checked_sub(1).and_then(at_rung)
}

/// The tier one step up the ladder (`sonnet/high` → `opus/low`).
pub fn step_up(t: Tier) -> Option<Tier> {
    at_rung(rung(t) + 1)
}

/// Counted results of one task kind on one tier.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Basis {
    pub samples: u32,
    pub successes: u32,
    pub failures: u32,
}

pub type Stats = HashMap<(String, Tier), Basis>;

/// Pure: the counted results per (task kind, tier). Rows of `other`, of no
/// known tier or not counted (QO-FR34) are left out.
pub fn stats(rows: &[DecisionRow]) -> Stats {
    let mut out = Stats::new();
    for row in rows {
        if row.task_kind == super::tier::OTHER {
            continue;
        }
        let Some((model, effort)) = report::row_tier(row) else { continue };
        let ok = match report::outcome(row) {
            Outcome::Success => true,
            Outcome::Failure => false,
            Outcome::NotCounted => continue,
        };
        let basis = out.entry((row.task_kind.clone(), Tier::new(model, effort))).or_default();
        basis.samples += 1;
        basis.successes += u32::from(ok);
        basis.failures += u32::from(!ok);
    }
    out
}

/// One task kind's move, with the counts it rests on (`None` for a revert).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub kind: String,
    pub from: Option<Tier>,
    pub to: Option<Tier>,
    pub basis: Option<Basis>,
}

fn at_or_above(t: Tier, floor: Option<Tier>) -> bool {
    floor.is_none_or(|f| rung(t) >= rung(f))
}

/// Pure (QO-FR45, QO-AC57 … QO-AC61): the moves of a recompute. A kind with no
/// tier (`other`) never moves; one without enough data stays.
pub fn recompute(table: &[KindRule], stats: &Stats) -> Vec<Change> {
    let basis = |kind: &str, tier: Tier| stats.get(&(kind.to_string(), tier)).copied().unwrap_or_default();
    let mut changes = Vec::new();
    for rule in table {
        let Some(current) = rule.tier else { continue };
        let here = basis(&rule.kind, current);
        let up = (here.samples >= UP_MIN_SAMPLES && here.failures * 100 >= UP_MIN_FAILURE_PCT * here.samples)
            .then(|| step_up(current))
            .flatten();
        let change = if let Some(to) = up {
            Some((to, here))
        } else {
            step_down(current).filter(|t| at_or_above(*t, rule.floor)).and_then(|to| {
                let below = basis(&rule.kind, to);
                (below.samples >= DOWN_MIN_SAMPLES && below.successes * 100 >= DOWN_MIN_SUCCESS_PCT * below.samples)
                    .then_some((to, below))
            })
        };
        if let Some((to, basis)) = change {
            changes.push(Change { kind: rule.kind.clone(), from: Some(current), to: Some(to), basis: Some(basis) });
        }
    }
    changes
}

/// The table with `changes` applied.
pub fn apply(table: &[KindRule], changes: &[Change]) -> Vec<KindRule> {
    table
        .iter()
        .map(|rule| match changes.iter().find(|c| c.kind == rule.kind) {
            Some(change) => KindRule { tier: change.to, ..rule.clone() },
            None => rule.clone(),
        })
        .collect()
}

/// The kinds whose tier differs from `from` to `to` (a revert's log).
pub fn diff(from: &[KindRule], to: &[KindRule]) -> Vec<Change> {
    to.iter()
        .filter_map(|rule| {
            let before = from.iter().find(|r| r.kind == rule.kind).and_then(|r| r.tier);
            (before != rule.tier).then(|| Change { kind: rule.kind.clone(), from: before, to: rule.tier, basis: None })
        })
        .collect()
}

/// Pure (QO-AC62): whether to recompute now. Forced ("Update now") always;
/// otherwise ≥ [`MIN_HOURS_BETWEEN`] since the last time (or never before) and
/// ≥ [`MIN_NEW_RESULTS`] counted results since.
pub fn should_recompute(last: Option<DateTime<Utc>>, new_results: usize, now: DateTime<Utc>, forced: bool) -> bool {
    forced
        || (last.is_none_or(|l| now - l >= chrono::Duration::hours(MIN_HOURS_BETWEEN)) && new_results >= MIN_NEW_RESULTS)
}

/// Changes found while [`AUTO_SETTING`] was off, against version `based_on`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub based_on: i64,
    pub created_at: String,
    pub changes: Vec<Change>,
}

/// What a recompute did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Recomputed {
    /// A gate was not met; nothing was read.
    Skipped,
    Unchanged,
    Created { version: i64, changes: Vec<Change> },
    Proposed { changes: Vec<Change> },
}

// One recompute or revert at a time (the scheduler and a button can meet).
static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn setting(db: &Db, key: &str) -> Option<String> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = ?").bind(key).fetch_optional(db).await.ok().flatten()
}

async fn set(db: &Db, key: &str, value: &str) -> Result<(), String> {
    crate::commands::settings::set_setting(db, key, value).await
}

async fn unset(db: &Db, key: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM settings WHERE key = ?").bind(key).execute(db).await.map_err(|e| e.to_string())?;
    Ok(())
}

async fn finished_since(db: &Db, since: &str) -> Result<Vec<DecisionRow>, String> {
    sqlx::query_as(&format!(
        "SELECT {} FROM routing_decisions WHERE finished_at IS NOT NULL AND finished_at > ?",
        report::ROW_COLUMNS
    ))
    .bind(since)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())
}

fn parse_table(json: &str) -> Result<Vec<KindRule>, String> {
    serde_json::from_str(json).map_err(|e| format!("routing policy table: {e}"))
}

async fn current_table(db: &Db) -> Result<(i64, Vec<KindRule>), String> {
    policy::ensure_seeded(db).await?;
    let (version, json): (i64, String) =
        sqlx::query_as("SELECT version, table_json FROM routing_policies ORDER BY version DESC LIMIT 1")
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string())?;
    Ok((version, parse_table(&json)?))
}

/// Write the next version: its text built from `table`, stored whole.
async fn create_version(db: &Db, source: &str, table: &[KindRule], changes: &[Change]) -> Result<i64, String> {
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    let version: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) + 1 FROM routing_policies")
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("INSERT INTO routing_policies (version, created_at, source, table_json, text, changes_json) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(version)
        .bind(Utc::now().to_rfc3339())
        .bind(source)
        .bind(serde_json::to_string(table).map_err(|e| e.to_string())?)
        .bind(policy::policy_text(version, table))
        .bind(serde_json::to_string(changes).map_err(|e| e.to_string())?)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(version)
}

/// Recompute the table at `now`, behind the gates unless `forced`, and act on
/// it (QO-AC61, QO-AC63, QO-AC64).
pub async fn run(db: &Db, forced: bool, now: DateTime<Utc>) -> Result<Recomputed, String> {
    let _one_at_a_time = LOCK.lock().await;
    let last_raw = setting(db, LAST_SETTING).await.unwrap_or_default();
    let last = DateTime::parse_from_rfc3339(last_raw.trim()).ok().map(|t| t.with_timezone(&Utc));
    let since = last.map(|t| t.to_rfc3339()).unwrap_or_default();
    let new_results = finished_since(db, &since).await?.iter().filter(|r| report::outcome(r) != Outcome::NotCounted).count();
    if !should_recompute(last, new_results, now, forced) {
        return Ok(Recomputed::Skipped);
    }
    let (version, table) = current_table(db).await?;
    let window = finished_since(db, &(now - chrono::Duration::days(WINDOW_DAYS)).to_rfc3339()).await?;
    let changes = recompute(&table, &stats(&window));
    set(db, LAST_SETTING, &now.to_rfc3339()).await?;
    if changes.is_empty() {
        // An older proposal no longer holds either.
        unset(db, PROPOSAL_SETTING).await?;
        return Ok(Recomputed::Unchanged);
    }
    if setting(db, AUTO_SETTING).await.as_deref().map(str::trim) == Some("false") {
        let proposal = Proposal { based_on: version, created_at: now.to_rfc3339(), changes: changes.clone() };
        set(db, PROPOSAL_SETTING, &serde_json::to_string(&proposal).map_err(|e| e.to_string())?).await?;
        return Ok(Recomputed::Proposed { changes });
    }
    let created = create_version(db, "auto", &apply(&table, &changes), &changes).await?;
    set(db, NOTICE_SETTING, &created.to_string()).await?;
    unset(db, PROPOSAL_SETTING).await?;
    Ok(Recomputed::Created { version: created, changes })
}

/// "Go back to this version" (QO-FR50, QO-AC65): a new version with the
/// chosen one's table (source `revert`), and automatic updates switched off so
/// the next recompute does not bring the dropped change straight back.
pub async fn revert(db: &Db, version: i64) -> Result<i64, String> {
    let _one_at_a_time = LOCK.lock().await;
    let json: Option<String> = sqlx::query_scalar("SELECT table_json FROM routing_policies WHERE version = ?")
        .bind(version)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    let target = parse_table(&json.ok_or_else(|| format!("routing policy version {version} not found"))?)?;
    let (_, current) = current_table(db).await?;
    let created = create_version(db, "revert", &target, &diff(&current, &target)).await?;
    set(db, AUTO_SETTING, "false").await?;
    unset(db, PROPOSAL_SETTING).await?;
    Ok(created)
}

/// One version for the log of Settings › Routing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PolicyVersion {
    pub version: i64,
    pub created_at: String,
    pub source: String,
    pub changes: Vec<Change>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Policies {
    /// Newest first; the first is the current one.
    pub versions: Vec<PolicyVersion>,
    pub proposal: Option<Proposal>,
}

pub async fn list(db: &Db) -> Result<Policies, String> {
    policy::ensure_seeded(db).await?;
    let rows: Vec<(i64, String, String, String, String)> = sqlx::query_as(
        "SELECT version, created_at, source, changes_json, text FROM routing_policies ORDER BY version DESC",
    )
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;
    let versions = rows
        .into_iter()
        .map(|(version, created_at, source, changes, text)| PolicyVersion {
            version,
            created_at,
            source,
            changes: serde_json::from_str(&changes).unwrap_or_default(),
            text,
        })
        .collect();
    let proposal = setting(db, PROPOSAL_SETTING).await.and_then(|json| serde_json::from_str(&json).ok());
    Ok(Policies { versions, proposal })
}

/// An automatic version to tell the user about once (QO-FR48).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notice {
    pub version: i64,
    pub changes: Vec<Change>,
}

/// The pending notice, cleared as it is read: the toast shows once, however
/// many windows or events ask.
pub async fn take_notice(db: &Db) -> Result<Option<Notice>, String> {
    let _one_at_a_time = LOCK.lock().await;
    let Some(raw) = setting(db, NOTICE_SETTING).await else { return Ok(None) };
    unset(db, NOTICE_SETTING).await?;
    let Ok(version) = raw.trim().parse::<i64>() else { return Ok(None) };
    let changes: Option<String> = sqlx::query_scalar("SELECT changes_json FROM routing_policies WHERE version = ?")
        .bind(version)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(changes.map(|json| Notice { version, changes: serde_json::from_str(&json).unwrap_or_default() }))
}

/// Recompute and, when a version was made, tell the windows.
pub async fn recompute_and_notify(app: &tauri::AppHandle, forced: bool) -> Result<Recomputed, String> {
    let db = app.state::<Db>().inner().clone();
    let result = run(&db, forced, Utc::now()).await?;
    if let Recomputed::Created { version, .. } = &result {
        tracing::info!(event = "routing_policy_updated", version = *version);
        let _ = app.emit(UPDATED_EVENT, serde_json::json!({ "version": version }));
    }
    Ok(result)
}

/// The background check, started once at app setup.
pub fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        loop {
            if let Err(e) = recompute_and_notify(&app, false).await {
                tracing::warn!(event = "routing_recompute_failed", error = %e);
            }
            tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::routing::report::tests::row;
    use Effort::*;
    use Model::*;

    fn rule(kind: &str, tier: Tier, floor: Tier) -> KindRule {
        KindRule { kind: kind.into(), tier: Some(tier), floor: Some(floor) }
    }

    fn stats_of(entries: &[(&str, Tier, u32, u32)]) -> Stats {
        entries
            .iter()
            .map(|(kind, tier, successes, failures)| {
                ((kind.to_string(), *tier), Basis { samples: successes + failures, successes: *successes, failures: *failures })
            })
            .collect()
    }

    #[test]
    fn the_ladder_has_nine_rungs_model_first() {
        assert_eq!(step_down(Tier::new(Sonnet, Medium)), Some(Tier::new(Sonnet, Low)));
        assert_eq!(step_down(Tier::new(Sonnet, Low)), Some(Tier::new(Haiku, High)));
        assert_eq!(step_down(Tier::new(Haiku, Low)), None);
        assert_eq!(step_up(Tier::new(Sonnet, High)), Some(Tier::new(Opus, Low)));
        assert_eq!(step_up(Tier::new(Opus, High)), None);
    }

    /// QO-AC57, QO-AC58.
    #[test]
    fn a_kind_steps_down_on_twenty_good_results_below() {
        let table = [rule("test", Tier::new(Sonnet, Medium), Tier::new(Sonnet, Low))];
        let changes = recompute(&table, &stats_of(&[("test", Tier::new(Sonnet, Low), 19, 1)]));
        assert_eq!(
            changes,
            vec![Change {
                kind: "test".into(),
                from: Some(Tier::new(Sonnet, Medium)),
                to: Some(Tier::new(Sonnet, Low)),
                basis: Some(Basis { samples: 20, successes: 19, failures: 1 }),
            }]
        );
        assert_eq!(apply(&table, &changes)[0].tier, Some(Tier::new(Sonnet, Low)));
        // 19 samples: not enough.
        assert!(recompute(&table, &stats_of(&[("test", Tier::new(Sonnet, Low), 18, 1)])).is_empty());
        // 20 samples under 90%: not good enough.
        assert!(recompute(&table, &stats_of(&[("test", Tier::new(Sonnet, Low), 17, 3)])).is_empty());
    }

    /// QO-AC59.
    #[test]
    fn a_kind_never_goes_below_its_floor() {
        let table = [rule("implement", Tier::new(Sonnet, Medium), Tier::new(Sonnet, Medium))];
        let stats = stats_of(&[
            ("implement", Tier::new(Haiku, Medium), 50, 0),
            ("implement", Tier::new(Sonnet, Low), 50, 0),
        ]);
        assert!(recompute(&table, &stats).is_empty());
    }

    /// QO-AC60, and up wins when both hold.
    #[test]
    fn a_kind_steps_up_once_on_too_many_failures() {
        let table = [rule("review", Tier::new(Sonnet, Medium), Tier::new(Sonnet, Medium))];
        let changes = recompute(&table, &stats_of(&[("review", Tier::new(Sonnet, Medium), 3, 2)]));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].to, Some(Tier::new(Sonnet, High)), "exactly one step");
        assert_eq!(changes[0].basis, Some(Basis { samples: 5, successes: 3, failures: 2 }));
        // 4 samples: not enough; 1 in 5 failed: under 30%.
        assert!(recompute(&table, &stats_of(&[("review", Tier::new(Sonnet, Medium), 2, 2)])).is_empty());
        assert!(recompute(&table, &stats_of(&[("review", Tier::new(Sonnet, Medium), 4, 1)])).is_empty());

        let docs = [rule("docs", Tier::new(Sonnet, Medium), Tier::new(Sonnet, Low))];
        let both = stats_of(&[("docs", Tier::new(Sonnet, Medium), 3, 3), ("docs", Tier::new(Sonnet, Low), 20, 0)]);
        assert_eq!(recompute(&docs, &both)[0].to, Some(Tier::new(Sonnet, High)));
        // At the top there is nowhere up; the good tier below still counts.
        let top = [rule("plan", Tier::new(Opus, High), Tier::new(Sonnet, Medium))];
        let stats = stats_of(&[("plan", Tier::new(Opus, High), 0, 5), ("plan", Tier::new(Opus, Medium), 20, 0)]);
        assert_eq!(recompute(&top, &stats)[0].to, Some(Tier::new(Opus, Medium)));
    }

    /// QO-AC61 (pure part): no data, no move; `other` never moves.
    #[test]
    fn without_enough_data_the_table_stays() {
        let table = policy::v1_table();
        assert!(recompute(&table, &Stats::new()).is_empty());
        assert_eq!(apply(&table, &[]), table);
        assert!(diff(&table, &table).is_empty());
    }

    #[test]
    fn stats_count_only_results_on_a_known_tier() {
        let done = |kind: &str, status: &str| report::DecisionRow { status: Some(status.into()), ..row("subagent", kind, Some("sonnet"), Some("low")) };
        let rows = vec![
            done("search", "completed"),
            done("search", "failed"),
            done("search", "stopped"),
            done("other", "completed"),
            row("subagent", "search", None, Some("low")),
            row("subagent", "search", Some("sonnet"), Some("max")),
        ];
        let s = stats(&rows);
        assert_eq!(s.len(), 1);
        assert_eq!(s[&("search".to_string(), Tier::new(Sonnet, Low))], Basis { samples: 2, successes: 1, failures: 1 });
    }

    /// QO-AC62.
    #[test]
    fn the_gates_need_a_day_and_twenty_results_unless_forced() {
        let now = Utc::now();
        let ten_hours = Some(now - chrono::Duration::hours(10));
        let a_day = Some(now - chrono::Duration::hours(24));
        assert!(!should_recompute(ten_hours, 50, now, false));
        assert!(!should_recompute(a_day, 19, now, false));
        assert!(should_recompute(a_day, 20, now, false));
        assert!(should_recompute(None, 20, now, false));
        assert!(!should_recompute(None, 0, now, false));
        assert!(should_recompute(ten_hours, 0, now, true), "Update now");
    }

    async fn db() -> Db {
        let db = crate::runs::routing::policy::tests::db().await;
        policy::ensure_seeded(&db).await.unwrap();
        db
    }

    /// `n` results of `kind` on `sonnet/low` finished at `at`, `ok` of them good.
    async fn results_at(db: &Db, kind: &str, n: usize, ok: usize, at: DateTime<Utc>) {
        let at = at.to_rfc3339();
        for i in 0..n {
            sqlx::query(
                "INSERT INTO routing_decisions (id, created_at, finished_at, kind, decided_by, task_kind, requested_model, requested_effort, status) \
                 VALUES (?, ?, ?, 'subagent', 'agent', ?, 'sonnet', 'low', ?)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&at)
            .bind(&at)
            .bind(kind)
            .bind(if i < ok { "completed" } else { "failed" })
            .execute(db)
            .await
            .unwrap();
        }
    }

    async fn results(db: &Db, kind: &str, n: usize, ok: usize) {
        results_at(db, kind, n, ok, Utc::now() - chrono::Duration::hours(1)).await;
    }

    async fn versions(db: &Db) -> Vec<(i64, String)> {
        sqlx::query_as("SELECT version, source FROM routing_policies ORDER BY version").fetch_all(db).await.unwrap()
    }

    /// QO-AC62 at startup, QO-AC61, then QO-AC63: a change makes an `auto`
    /// version new sessions bind to, and one notice.
    #[tokio::test]
    async fn an_automatic_update_makes_a_version_and_one_notice() {
        let db = db().await;
        let now = Utc::now();
        results(&db, "test", 10, 10).await;
        assert_eq!(run(&db, false, now).await.unwrap(), Recomputed::Skipped, "under 20 new results");
        assert_eq!(setting(&db, LAST_SETTING).await, None);
        // `test` is on sonnet/medium; 10 good results on sonnet/low are not enough to move it.
        assert_eq!(run(&db, true, now).await.unwrap(), Recomputed::Unchanged);
        assert_eq!(versions(&db).await.len(), 1);
        assert!(setting(&db, LAST_SETTING).await.is_some());

        // 20 more since then, 19 good: sonnet/low now has 30 results, 29 good.
        results_at(&db, "test", 20, 19, now + chrono::Duration::hours(1)).await;
        assert_eq!(run(&db, false, now + chrono::Duration::hours(10)).await.unwrap(), Recomputed::Skipped, "10 hours after the last time");
        let Recomputed::Created { version, changes } = run(&db, false, now + chrono::Duration::hours(25)).await.unwrap() else {
            panic!("expected a new version");
        };
        assert_eq!(version, 2);
        assert_eq!(changes[0].to, Some(Tier::new(Sonnet, Low)));
        assert_eq!(versions(&db).await, vec![(1, "default".into()), (2, "auto".into())]);
        let text = policy::text_of(&db, 2).await.unwrap();
        assert!(text.starts_with("<routing-policy version=\"2\">") && text.contains("- test: sonnet / low\n"));
        assert_eq!(policy::current(&db).await.map(|(v, _)| v), Some(2), "new sessions bind to it");

        let notice = take_notice(&db).await.unwrap().unwrap();
        assert_eq!((notice.version, notice.changes), (2, changes));
        assert_eq!(take_notice(&db).await.unwrap(), None, "shown once");
    }

    /// QO-AC64.
    #[tokio::test]
    async fn with_automatic_updates_off_the_change_is_a_proposal() {
        let db = db().await;
        set(&db, AUTO_SETTING, "false").await.unwrap();
        results(&db, "docs", 20, 20).await;
        let Recomputed::Proposed { changes } = run(&db, false, Utc::now()).await.unwrap() else {
            panic!("expected a proposal");
        };
        assert_eq!(changes[0].kind, "docs");
        assert_eq!(versions(&db).await.len(), 1, "no new version");
        assert_eq!(take_notice(&db).await.unwrap(), None);
        let listed = list(&db).await.unwrap();
        assert_eq!(listed.proposal.map(|p| (p.based_on, p.changes)), Some((1, changes)));
        assert_eq!(listed.versions.len(), 1);
    }

    /// QO-AC65.
    #[tokio::test]
    async fn going_back_copies_the_table_and_switches_automatic_updates_off() {
        let db = db().await;
        results(&db, "test", 20, 20).await;
        let Recomputed::Created { version: v2, .. } = run(&db, true, Utc::now()).await.unwrap() else { panic!() };
        results(&db, "docs", 20, 20).await;
        let Recomputed::Created { version: v3, .. } = run(&db, true, Utc::now()).await.unwrap() else { panic!() };
        assert_eq!((v2, v3), (2, 3));
        set(&db, PROPOSAL_SETTING, "{}").await.unwrap();

        let v4 = revert(&db, 2).await.unwrap();
        assert_eq!(v4, 4);
        let table = |v: i64| {
            let db = db.clone();
            async move {
                sqlx::query_scalar::<_, String>("SELECT table_json FROM routing_policies WHERE version = ?")
                    .bind(v)
                    .fetch_one(&db)
                    .await
                    .unwrap()
            }
        };
        assert_eq!(table(4).await, table(2).await);
        assert_eq!(versions(&db).await.last(), Some(&(4, "revert".to_string())));
        assert_eq!(setting(&db, AUTO_SETTING).await.as_deref(), Some("false"));
        assert_eq!(setting(&db, PROPOSAL_SETTING).await, None);
        let listed = list(&db).await.unwrap();
        assert_eq!(listed.versions[0].version, 4);
        assert_eq!(listed.versions[0].changes.iter().map(|c| c.kind.as_str()).collect::<Vec<_>>(), vec!["docs"]);
        assert!(listed.versions[0].changes[0].basis.is_none());
        assert!(revert(&db, 99).await.is_err());
    }

    #[test]
    fn the_auto_switch_is_portable_and_bookkeeping_is_not() {
        use crate::commands::settings::is_portable_setting;
        assert!(is_portable_setting(AUTO_SETTING));
        assert!(crate::remote::guard::remote_may_set_setting(AUTO_SETTING));
        for key in [LAST_SETTING, PROPOSAL_SETTING, NOTICE_SETTING] {
            assert!(!is_portable_setting(key), "{key}");
            assert!(!crate::remote::guard::remote_may_set_setting(key), "{key}");
        }
    }

    #[test]
    fn going_back_and_updating_now_are_audited_and_reads_are_not() {
        use crate::remote::guard::{audited_command, is_local_only_command};
        for name in ["recompute_routing_policy", "revert_routing_policy"] {
            assert!(audited_command(name) && !is_local_only_command(name), "{name}");
        }
        for name in ["get_routing_report", "list_routing_policies", "take_routing_notice"] {
            assert!(!audited_command(name) && !is_local_only_command(name), "{name}");
        }
    }
}
