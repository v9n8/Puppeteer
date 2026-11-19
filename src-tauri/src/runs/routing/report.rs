//! Layer 3a, read side: the routing report of Settings › Routing (QO-FR36,
//! QO-AC49) and the success / failure of a recorded decision (QO-FR34), which
//! the policy update counts as well. Built in Rust from the rows of a period
//! (a few thousand a month at most), so the rules stay pure and tested; no
//! model is called.

use serde::Serialize;

use super::tier::{Effort, Model};
use crate::conductor::defaults::TASK_KINDS;
use crate::db::Db;

/// The columns of `routing_decisions` the report and the update read.
#[derive(Debug, Clone, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct DecisionRow {
    pub kind: String,
    pub decided_by: String,
    pub task_kind: String,
    pub requested_model: Option<String>,
    pub resolved_model: Option<String>,
    pub requested_effort: Option<String>,
    pub status: Option<String>,
    pub verification: Option<String>,
    pub total_tokens: Option<i64>,
    pub escalated: i64,
    pub policy_version: Option<i64>,
}

pub const ROW_COLUMNS: &str = "kind, decided_by, task_kind, requested_model, resolved_model, requested_effort, \
     status, verification, total_tokens, escalated, policy_version";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Success,
    Failure,
    NotCounted,
}

/// QO-FR34. A worker: its check passed → success; it failed, or the work was
/// retried one tier up → failure; no check said, or the user stopped it → not
/// counted. A sub-agent: retried or failed → failure; completed → success;
/// stopped or still open → not counted.
pub fn outcome(row: &DecisionRow) -> Outcome {
    if row.escalated != 0 {
        return Outcome::Failure;
    }
    let status = row.status.as_deref();
    if row.kind == "worker" {
        if status == Some("stopped") {
            return Outcome::NotCounted;
        }
        return match row.verification.as_deref() {
            Some("pass") => Outcome::Success,
            Some("fail") => Outcome::Failure,
            _ => Outcome::NotCounted,
        };
    }
    match status {
        Some("completed") => Outcome::Success,
        Some("failed") => Outcome::Failure,
        _ => Outcome::NotCounted,
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

/// The model a row ran on, as a family: the model that ran, else the one asked
/// for. A model of no Claude family (a Codex worker) is shown as stored; `""`
/// when neither is known (the engine picked).
pub fn model_label(row: &DecisionRow) -> String {
    let named = non_empty(row.resolved_model.as_deref()).or(non_empty(row.requested_model.as_deref()));
    match named {
        Some(m) => Model::family(m).map_or_else(|| m.to_string(), |f| f.as_str().to_string()),
        None => String::new(),
    }
}

/// The row's tier on the routing ladder, when both its model family and a
/// routed effort are known.
pub fn row_tier(row: &DecisionRow) -> Option<(Model, Effort)> {
    let model = Model::family(&model_label(row))?;
    let effort = Effort::parse(non_empty(row.requested_effort.as_deref())?)?;
    Some((model, effort))
}

/// One line of the report: a task kind on one tier.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Group {
    pub task_kind: String,
    /// Model family (or id); `""` = not known.
    pub model: String,
    /// `""` = not set.
    pub effort: String,
    pub runs: u32,
    /// Runs with a success or a failure (the base of the success rate).
    pub counted: u32,
    pub successes: u32,
    /// Mean of the runs that recorded tokens; `None` when none did.
    pub avg_tokens: Option<u64>,
    /// Runs retried one tier up.
    pub retried: u32,
    /// Runs whose model or effort the agent chose.
    pub by_agent: u32,
}

/// QO-NFR13: the `Agent` calls of sessions that had a policy, and how many
/// named a model, an effort, both.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Adoption {
    pub calls: u32,
    pub with_model: u32,
    pub with_effort: u32,
    pub with_both: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Report {
    pub days: u32,
    pub groups: Vec<Group>,
    pub adoption: Adoption,
}

fn kind_rank(kind: &str) -> usize {
    TASK_KINDS.iter().position(|k| *k == kind).unwrap_or(TASK_KINDS.len())
}

fn model_rank(model: &str) -> u8 {
    match Model::family(model) {
        Some(Model::Haiku) => 0,
        Some(Model::Sonnet) => 1,
        Some(Model::Opus) => 2,
        None if model.is_empty() => 4,
        None => 3,
    }
}

fn effort_rank(effort: &str) -> u8 {
    Effort::parse(effort).map_or(if effort.is_empty() { 4 } else { 3 }, |e| e as u8)
}

/// Pure: the report of `rows` (already cut to the period). Groups go in the
/// task kinds' order, then the tier from cheapest; unknown parts last.
pub fn build(rows: &[DecisionRow], days: u32) -> Report {
    let mut groups: Vec<Group> = Vec::new();
    let mut tokens: Vec<(u64, u64)> = Vec::new();
    let mut adoption = Adoption::default();
    for row in rows {
        let model = model_label(row);
        let effort = non_empty(row.requested_effort.as_deref()).unwrap_or_default().to_string();
        let at = match groups.iter().position(|g| g.task_kind == row.task_kind && g.model == model && g.effort == effort) {
            Some(i) => i,
            None => {
                groups.push(Group { task_kind: row.task_kind.clone(), model, effort, ..Group::default() });
                tokens.push((0, 0));
                groups.len() - 1
            }
        };
        let group = &mut groups[at];
        group.runs += 1;
        match outcome(row) {
            Outcome::Success => {
                group.counted += 1;
                group.successes += 1;
            }
            Outcome::Failure => group.counted += 1,
            Outcome::NotCounted => {}
        }
        if row.escalated != 0 {
            group.retried += 1;
        }
        if row.decided_by == "agent" {
            group.by_agent += 1;
        }
        if let Some(t) = row.total_tokens.filter(|t| *t >= 0) {
            tokens[at].0 += t as u64;
            tokens[at].1 += 1;
        }
        if row.kind == "subagent" && row.policy_version.is_some_and(|v| v > 0) {
            let has_model = non_empty(row.requested_model.as_deref()).is_some();
            let has_effort = non_empty(row.requested_effort.as_deref()).is_some();
            adoption.calls += 1;
            adoption.with_model += u32::from(has_model);
            adoption.with_effort += u32::from(has_effort);
            adoption.with_both += u32::from(has_model && has_effort);
        }
    }
    for (group, (sum, n)) in groups.iter_mut().zip(tokens) {
        group.avg_tokens = (n > 0).then(|| (sum + n / 2) / n);
    }
    groups.sort_by(|a, b| {
        (kind_rank(&a.task_kind), &a.task_kind, model_rank(&a.model), &a.model, effort_rank(&a.effort), &a.effort).cmp(&(
            kind_rank(&b.task_kind),
            &b.task_kind,
            model_rank(&b.model),
            &b.model,
            effort_rank(&b.effort),
            &b.effort,
        ))
    });
    Report { days, groups, adoption }
}

/// The periods Settings › Routing offers.
pub const PERIODS: [u32; 2] = [7, 30];

/// The report of the last `days` days (rows made since then).
pub async fn load(db: &Db, days: u32) -> Result<Report, String> {
    if !PERIODS.contains(&days) {
        return Err(format!("days must be one of {PERIODS:?}"));
    }
    let since = (chrono::Utc::now() - chrono::Duration::days(i64::from(days))).to_rfc3339();
    let rows: Vec<DecisionRow> =
        sqlx::query_as(&format!("SELECT {ROW_COLUMNS} FROM routing_decisions WHERE created_at >= ? ORDER BY created_at"))
            .bind(since)
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())?;
    Ok(build(&rows, days))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn row(kind: &str, task_kind: &str, model: Option<&str>, effort: Option<&str>) -> DecisionRow {
        DecisionRow {
            kind: kind.into(),
            decided_by: "agent".into(),
            task_kind: task_kind.into(),
            requested_model: model.map(Into::into),
            resolved_model: None,
            requested_effort: effort.map(Into::into),
            status: Some("completed".into()),
            verification: None,
            total_tokens: None,
            escalated: 0,
            policy_version: Some(1),
        }
    }

    #[test]
    fn success_and_failure_follow_the_kind_of_work() {
        let worker = |verification: Option<&str>, status: &str| DecisionRow {
            verification: verification.map(Into::into),
            status: Some(status.into()),
            ..row("worker", "test", None, None)
        };
        assert_eq!(outcome(&worker(Some("pass"), "completed")), Outcome::Success);
        assert_eq!(outcome(&worker(Some("fail"), "completed")), Outcome::Failure);
        assert_eq!(outcome(&worker(Some("pass"), "failed")), Outcome::Success, "the check decides, not the turn");
        for none in [Some("none"), Some("missing"), None] {
            assert_eq!(outcome(&worker(none, "completed")), Outcome::NotCounted);
        }
        assert_eq!(outcome(&worker(Some("pass"), "stopped")), Outcome::NotCounted, "the user stopped it");
        assert_eq!(outcome(&DecisionRow { escalated: 1, ..worker(Some("fail"), "completed") }), Outcome::Failure);

        let call = |status: Option<&str>| DecisionRow { status: status.map(Into::into), ..row("subagent", "search", None, None) };
        assert_eq!(outcome(&call(Some("completed"))), Outcome::Success);
        assert_eq!(outcome(&call(Some("failed"))), Outcome::Failure);
        assert_eq!(outcome(&call(Some("stopped"))), Outcome::NotCounted);
        assert_eq!(outcome(&call(None)), Outcome::NotCounted, "still open");
        assert_eq!(outcome(&DecisionRow { escalated: 1, ..call(Some("completed")) }), Outcome::Failure);
    }

    #[test]
    fn the_model_is_the_one_that_ran_as_a_family() {
        let mut r = row("subagent", "search", Some("sonnet"), Some("low"));
        assert_eq!(model_label(&r), "sonnet");
        r.resolved_model = Some("claude-haiku-5-5".into());
        assert_eq!(model_label(&r), "haiku", "the model that ran wins");
        assert_eq!(row_tier(&r), Some((Model::Haiku, Effort::Low)));
        let codex = DecisionRow { resolved_model: Some("gpt-5-codex".into()), ..row("worker", "implement", None, None) };
        assert_eq!(model_label(&codex), "gpt-5-codex");
        assert_eq!(row_tier(&codex), None);
        assert_eq!(model_label(&row("subagent", "other", None, None)), "");
        assert_eq!(row_tier(&row("subagent", "docs", Some("opus"), Some("max"))), None, "an effort off the ladder");
    }

    /// QO-AC49: runs, success, average tokens, retries and the agent's choices
    /// per task kind × tier, from sample decisions.
    #[test]
    fn the_report_groups_by_kind_and_tier() {
        let mut rows = Vec::new();
        // test on sonnet / medium: 3 completed (one retried), 1 failed, 1 stopped; tokens on 2.
        for (status, escalated, tokens, by) in [
            ("completed", 0, Some(1000), "agent"),
            ("completed", 0, Some(2001), "agent"),
            ("completed", 1, None, "default"),
            ("failed", 0, None, "agent"),
            ("stopped", 0, None, "default"),
        ] {
            rows.push(DecisionRow {
                status: Some(status.into()),
                escalated,
                total_tokens: tokens,
                decided_by: by.into(),
                ..row("subagent", "test", Some("sonnet"), Some("medium"))
            });
        }
        // Its retry on opus / medium, and a worker for review.
        rows.push(DecisionRow { decided_by: "escalation".into(), total_tokens: Some(500), ..row("subagent", "test", Some("opus"), Some("medium")) });
        rows.push(DecisionRow {
            verification: Some("pass".into()),
            resolved_model: Some("claude-opus-5-5".into()),
            ..row("worker", "review", None, Some("medium"))
        });
        // A call that named nothing, in a session with no policy.
        rows.push(DecisionRow { decided_by: "default".into(), policy_version: Some(0), ..row("subagent", "other", None, None) });
        // search comes before test in the kinds' order.
        rows.push(row("subagent", "search", Some("haiku"), Some("low")));

        let report = build(&rows, 7);
        assert_eq!(report.days, 7);
        let keys: Vec<(&str, &str, &str)> =
            report.groups.iter().map(|g| (g.task_kind.as_str(), g.model.as_str(), g.effort.as_str())).collect();
        assert_eq!(
            keys,
            vec![
                ("search", "haiku", "low"),
                ("review", "opus", "medium"),
                ("test", "sonnet", "medium"),
                ("test", "opus", "medium"),
                ("other", "", ""),
            ]
        );
        let test = &report.groups[2];
        assert_eq!(
            *test,
            Group {
                task_kind: "test".into(),
                model: "sonnet".into(),
                effort: "medium".into(),
                runs: 5,
                counted: 4,
                successes: 2,
                avg_tokens: Some(1501),
                retried: 1,
                by_agent: 3,
            }
        );
        assert_eq!((report.groups[1].counted, report.groups[1].successes), (1, 1));
        assert_eq!(report.groups[3].by_agent, 0, "a retry is not the agent's own choice");
        assert_eq!(report.groups[4].avg_tokens, None);
    }

    /// QO-NFR13: only the `Agent` calls of sessions bound to a policy count.
    #[test]
    fn adoption_counts_calls_of_sessions_with_a_policy() {
        let rows = vec![
            row("subagent", "search", Some("sonnet"), Some("low")),
            row("subagent", "search", Some("sonnet"), None),
            row("subagent", "search", None, Some("low")),
            row("subagent", "other", Some(" "), None),
            DecisionRow { policy_version: Some(0), ..row("subagent", "search", None, None) },
            DecisionRow { policy_version: None, ..row("subagent", "search", None, None) },
            row("worker", "implement", Some("opus"), Some("medium")),
        ];
        assert_eq!(build(&rows, 30).adoption, Adoption { calls: 4, with_model: 2, with_effort: 2, with_both: 1 });
        assert_eq!(build(&[], 30), Report { days: 30, ..Report::default() });
    }

    #[tokio::test]
    async fn the_report_reads_the_period_only() {
        let db = crate::runs::routing::policy::tests::db().await;
        let at = |days_ago: i64| (chrono::Utc::now() - chrono::Duration::days(days_ago)).to_rfc3339();
        for (id, created) in [("a", at(1)), ("b", at(10)), ("c", at(40))] {
            sqlx::query(
                "INSERT INTO routing_decisions (id, created_at, kind, decided_by, task_kind, requested_model, requested_effort, status) \
                 VALUES (?, ?, 'subagent', 'agent', 'docs', 'sonnet', 'medium', 'completed')",
            )
            .bind(id)
            .bind(created)
            .execute(&db)
            .await
            .unwrap();
        }
        assert_eq!(load(&db, 7).await.unwrap().groups[0].runs, 1);
        assert_eq!(load(&db, 30).await.unwrap().groups[0].runs, 2);
        assert!(load(&db, 365).await.is_err());
    }
}
