//! Scheduled prompts (`orchestration.schedules`, report §5.2.4): open a new
//! session in a project and send it a prompt on a timer — every N minutes,
//! daily, on some weekdays, or once — only while the app is open.
//!
//! - [`Timing`], [`next_run`] (pure): when a schedule comes due next.
//! - [`runner`]: the 30 s loop. It fires through `start_run_inner`, the path
//!   every run takes (credential broker + shim, no token in the env, Rewind
//!   snapshot), and does nothing while [`FEATURE`] is off.
//! - [`commands`]: list / save / delete / switch on, for the Schedules screen.
//!
//! A scheduled run has nobody watching it (AWOG `schedule-propose-tool.ts`),
//! so only the person at this machine starts one: an agent can only propose
//! a schedule (MCP tool `schedule_propose`: `proposed = 1`, off, never fires
//! until saved from the screen), switching one on is local-only, and a save
//! from a remote desktop switches it off.

pub mod commands;
pub mod runner;

use crate::db::Db;
use chrono::{DateTime, Datelike, Duration, NaiveDateTime, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

pub const FEATURE: &str = "orchestration.schedules";
/// Emitted after every write, so an open Schedules screen reloads.
pub const CHANGED_EVENT: &str = "schedules:changed";

/// Longest interval: a week (longer is a weekly timing).
pub const MAX_INTERVAL_MINUTES: u32 = 7 * 24 * 60;
/// Modes a schedule may run in. `plan` (read and think, no writes) is the
/// default and the only one an agent proposes. In `default` a tool that needs
/// approval waits for someone to answer, which blocks the next firings.
pub const PERMISSION_MODES: &[&str] = &["plan", "default", "acceptEdits", "bypassPermissions"];
const ENGINES: &[&str] = &["claude", "codex"];
const MAX_NAME_CHARS: usize = 200;
const MAX_PROMPT_CHARS: usize = 20_000;
/// Days scanned for the next daily / weekly slot (weekly needs at most 8):
/// a broken timing ends as `None` instead of a long loop.
const MAX_DAY_SCAN: i64 = 14;

/// When a schedule fires, in the machine's local time. Stored as JSON in
/// `schedules.timing`; the MCP tool writes the same shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Timing {
    Interval { every_minutes: u32 },
    /// `HH:MM`, every day.
    Daily { time: String },
    /// `HH:MM` on these days, 0 = Sunday … 6 = Saturday.
    Weekly { weekdays: Vec<u8>, time: String },
    /// `YYYY-MM-DDTHH:MM`, one time only.
    Once { at: String },
}

impl Timing {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Timing::Interval { every_minutes } if !(1..=MAX_INTERVAL_MINUTES).contains(every_minutes) => {
                Err(format!("every_minutes must be 1–{MAX_INTERVAL_MINUTES}"))
            }
            Timing::Interval { .. } => Ok(()),
            Timing::Daily { time } => time_of_day(time).map(|_| ()).ok_or_else(|| bad_time(time)),
            Timing::Weekly { weekdays, time } => {
                if weekdays.is_empty() || weekdays.iter().any(|d| *d > 6) {
                    return Err("weekdays must be 0 (Sunday) … 6 (Saturday), at least one".into());
                }
                time_of_day(time).map(|_| ()).ok_or_else(|| bad_time(time))
            }
            Timing::Once { at } => once_at(at)
                .map(|_| ())
                .ok_or_else(|| format!("invalid date and time \"{at}\": expected YYYY-MM-DDTHH:MM")),
        }
    }
}

fn bad_time(time: &str) -> String {
    format!("invalid time \"{time}\": expected HH:MM (24 h)")
}

fn time_of_day(s: &str) -> Option<NaiveTime> {
    (s.len() == 5).then(|| NaiveTime::parse_from_str(s, "%H:%M").ok()).flatten()
}

fn once_at(s: &str) -> Option<NaiveDateTime> {
    (s.len() == 16).then(|| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").ok()).flatten()
}

/// `naive` on the wall clock of `tz`: the first of the two on a repeated hour,
/// an hour later in a skipped one (a 02:30 slot runs at 03:30 the night clocks
/// go forward instead of not at all).
fn at_local<Tz: TimeZone>(tz: &Tz, naive: NaiveDateTime) -> Option<DateTime<Tz>> {
    tz.from_local_datetime(&naive)
        .earliest()
        .or_else(|| tz.from_local_datetime(&(naive + Duration::hours(1))).earliest())
}

/// The first moment strictly after `from` that `timing` comes due, `None`
/// when nothing comes next (a past `once`, an invalid timing). Always after
/// `from`, so the slot that just fired is never returned again, and missed
/// slots collapse into one: the loop re-anchors from the time it fired.
pub fn next_run<Tz: TimeZone>(timing: &Timing, from: &DateTime<Tz>) -> Option<DateTime<Utc>> {
    timing.validate().ok()?;
    let tz = from.timezone();
    match timing {
        Timing::Interval { every_minutes } => {
            Some(from.with_timezone(&Utc) + Duration::minutes(i64::from(*every_minutes)))
        }
        Timing::Once { at } => at_local(&tz, once_at(at)?)
            .filter(|t| t > from)
            .map(|t| t.with_timezone(&Utc)),
        Timing::Daily { time } | Timing::Weekly { time, .. } => {
            let time = time_of_day(time)?;
            let days = match timing {
                Timing::Weekly { weekdays, .. } => Some(weekdays),
                _ => None,
            };
            let today = from.date_naive();
            (0..=MAX_DAY_SCAN).find_map(|offset| {
                let t = at_local(&tz, (today + Duration::days(offset)).and_time(time))?;
                // The day of the slot found, not of the day scanned: a skipped hour
                // can push it past midnight.
                let day = t.weekday().num_days_from_sunday() as u8;
                (t > *from && days.map_or(true, |d| d.contains(&day))).then(|| t.with_timezone(&Utc))
            })
        }
    }
}

/// How instants are stored (`next_run_at`, `last_run_at`).
fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn parse_stamp(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|t| t.with_timezone(&Utc))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Schedule {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub prompt: String,
    pub engine: Option<String>,
    pub permission_mode: String,
    /// `None` when the stored JSON does not parse: shown, never due.
    pub timing: Option<Timing>,
    pub enabled: bool,
    pub proposed: bool,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub last_run_id: Option<String>,
    pub last_status: Option<String>,
    pub last_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

const COLUMNS: &str = "id, project_id, name, prompt, engine, permission_mode, timing, enabled, proposed, \
     next_run_at, last_run_at, last_run_id, last_status, last_message, created_at, updated_at";

fn from_row(r: &sqlx::sqlite::SqliteRow) -> Schedule {
    Schedule {
        id: r.get("id"),
        project_id: r.get("project_id"),
        name: r.get("name"),
        prompt: r.get("prompt"),
        engine: r.get("engine"),
        permission_mode: r.get("permission_mode"),
        timing: serde_json::from_str(r.get::<&str, _>("timing")).ok(),
        enabled: r.get::<i64, _>("enabled") != 0,
        proposed: r.get::<i64, _>("proposed") != 0,
        next_run_at: r.get("next_run_at"),
        last_run_at: r.get("last_run_at"),
        last_run_id: r.get("last_run_id"),
        last_status: r.get("last_status"),
        last_message: r.get("last_message"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

pub async fn list(db: &Db) -> Result<Vec<Schedule>, String> {
    let rows = sqlx::query(&format!("SELECT {COLUMNS} FROM schedules ORDER BY created_at, id"))
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows.iter().map(from_row).collect())
}

pub async fn get(db: &Db, id: &str) -> Result<Schedule, String> {
    sqlx::query(&format!("SELECT {COLUMNS} FROM schedules WHERE id = ?"))
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .map(|r| from_row(&r))
        .ok_or_else(|| format!("Schedule not found: {id}"))
}

/// A schedule as the screen sends it. No `enabled`: only
/// [`set_enabled`] switches one on.
#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleInput {
    /// `None` creates one.
    pub id: Option<String>,
    pub project_id: String,
    pub name: String,
    pub prompt: String,
    pub engine: Option<String>,
    pub permission_mode: String,
    pub timing: Timing,
}

fn validate(input: &ScheduleInput) -> Result<(), String> {
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("name must be 1–{MAX_NAME_CHARS} characters"));
    }
    let prompt = input.prompt.trim();
    if prompt.is_empty() || prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(format!("prompt must be 1–{MAX_PROMPT_CHARS} characters"));
    }
    if let Some(e) = &input.engine {
        if !ENGINES.contains(&e.as_str()) {
            return Err(format!("Unknown engine: {e}"));
        }
    }
    if !PERMISSION_MODES.contains(&input.permission_mode.as_str()) {
        return Err(format!("Unknown permission mode: {}", input.permission_mode));
    }
    input.timing.validate()
}

/// Create or update a schedule. A save is the review of an agent's proposal
/// (`proposed` goes back to 0). An enabled schedule stays on, with its next
/// slot computed again from `now`, unless the save comes `from_remote`: then
/// it is switched off until someone at this machine switches it back on.
pub async fn save<Tz: TimeZone>(
    db: &Db,
    input: ScheduleInput,
    from_remote: bool,
    now: &DateTime<Tz>,
) -> Result<Schedule, String> {
    validate(&input)?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE id = ?)")
        .bind(&input.project_id)
        .fetch_one(db)
        .await
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err(format!("Project not found: {}", input.project_id));
    }
    let timing = serde_json::to_string(&input.timing).map_err(|e| e.to_string())?;
    let updated_at = Utc::now().to_rfc3339();
    let engine = input.engine.as_deref();
    let id = match input.id {
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO schedules (id, project_id, name, prompt, engine, permission_mode, timing, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&input.project_id)
            .bind(input.name.trim())
            .bind(input.prompt.trim())
            .bind(engine)
            .bind(&input.permission_mode)
            .bind(&timing)
            .bind(&updated_at)
            .bind(&updated_at)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
            id
        }
        Some(id) => {
            let old = get(db, &id).await?;
            let next = (old.enabled && !from_remote).then(|| next_run(&input.timing, now)).flatten();
            sqlx::query(
                "UPDATE schedules SET project_id = ?, name = ?, prompt = ?, engine = ?, permission_mode = ?, timing = ?,
                    proposed = 0, enabled = ?, next_run_at = ?, updated_at = ?
                 WHERE id = ?",
            )
            .bind(&input.project_id)
            .bind(input.name.trim())
            .bind(input.prompt.trim())
            .bind(engine)
            .bind(&input.permission_mode)
            .bind(&timing)
            .bind(next.is_some())
            .bind(next.map(stamp))
            .bind(&updated_at)
            .bind(&id)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
            id
        }
    };
    get(db, &id).await
}

/// Switch a schedule on (next slot from `now`) or off. A proposal must be
/// reviewed and saved first; a schedule with nothing to come (a past `once`)
/// cannot be switched on.
pub async fn set_enabled<Tz: TimeZone>(
    db: &Db,
    id: &str,
    enabled: bool,
    now: &DateTime<Tz>,
) -> Result<Schedule, String> {
    let s = get(db, id).await?;
    let next = if enabled {
        if s.proposed {
            return Err("This schedule was proposed by an agent: open it, review it and save it first.".into());
        }
        let next = s.timing.as_ref().and_then(|t| next_run(t, now));
        Some(next.ok_or("This schedule has no time left to run (a one-time schedule in the past?).")?)
    } else {
        None
    };
    sqlx::query("UPDATE schedules SET enabled = ?, next_run_at = ?, updated_at = ? WHERE id = ?")
        .bind(enabled)
        .bind(next.map(stamp))
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    get(db, id).await
}

pub async fn delete(db: &Db, id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM schedules WHERE id = ?")
        .bind(id)
        .execute(db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use chrono::FixedOffset;

    /// Hanoi time: no daylight saving, so the wall clock is fixed.
    pub fn at(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    fn utc(s: &str) -> Option<DateTime<Utc>> {
        Some(at(s).with_timezone(&Utc))
    }

    fn daily(time: &str) -> Timing {
        Timing::Daily { time: time.into() }
    }

    #[test]
    fn interval_counts_from_now() {
        let t = Timing::Interval { every_minutes: 90 };
        assert_eq!(next_run(&t, &at("2026-11-08T23:00:00+07:00")), utc("2026-11-09T00:30:00+07:00"));
    }

    #[test]
    fn daily_is_today_until_the_time_passes() {
        let t = daily("08:30");
        assert_eq!(next_run(&t, &at("2026-11-08T08:00:00+07:00")), utc("2026-11-08T08:30:00+07:00"));
        // The slot that just fired is not returned again.
        assert_eq!(next_run(&t, &at("2026-11-08T08:30:00+07:00")), utc("2026-11-09T08:30:00+07:00"));
        // Month end rolls over.
        assert_eq!(next_run(&t, &at("2026-11-30T09:00:00+07:00")), utc("2026-12-01T08:30:00+07:00"));
    }

    #[test]
    fn weekly_picks_the_next_listed_day() {
        // 2026-11-08 is a Sunday.
        let t = Timing::Weekly { weekdays: vec![1, 5], time: "09:00".into() };
        assert_eq!(next_run(&t, &at("2026-11-08T12:00:00+07:00")), utc("2026-11-09T09:00:00+07:00"));
        assert_eq!(next_run(&t, &at("2026-11-09T09:00:00+07:00")), utc("2026-11-13T09:00:00+07:00"));
        let sunday = Timing::Weekly { weekdays: vec![0], time: "07:00".into() };
        assert_eq!(next_run(&sunday, &at("2026-11-08T08:00:00+07:00")), utc("2026-11-15T07:00:00+07:00"));
    }

    #[test]
    fn once_runs_only_while_it_is_ahead() {
        let t = Timing::Once { at: "2026-11-09T07:15".into() };
        assert_eq!(next_run(&t, &at("2026-11-08T12:00:00+07:00")), utc("2026-11-09T07:15:00+07:00"));
        assert_eq!(next_run(&t, &at("2026-11-09T07:15:00+07:00")), None);
    }

    #[test]
    fn local_time_is_the_wall_clock_of_now() {
        // The same timing and instant, seen from another time zone.
        let utc_now = at("2026-11-08T01:00:00+00:00");
        assert_eq!(next_run(&daily("08:30"), &utc_now), utc("2026-11-08T08:30:00+00:00"));
        let hanoi_now = utc_now.with_timezone(&FixedOffset::east_opt(7 * 3600).unwrap());
        assert_eq!(next_run(&daily("08:30"), &hanoi_now), utc("2026-11-08T08:30:00+07:00"));
    }

    #[test]
    fn invalid_timings_never_come_due() {
        let now = at("2026-11-08T12:00:00+07:00");
        for t in [
            Timing::Interval { every_minutes: 0 },
            Timing::Interval { every_minutes: MAX_INTERVAL_MINUTES + 1 },
            daily("8:30"),
            daily("24:00"),
            daily("08:30:00"),
            Timing::Weekly { weekdays: vec![], time: "08:30".into() },
            Timing::Weekly { weekdays: vec![7], time: "08:30".into() },
            Timing::Once { at: "2026-11-09 07:15".into() },
            Timing::Once { at: "tomorrow".into() },
        ] {
            assert!(t.validate().is_err(), "{t:?}");
            assert_eq!(next_run(&t, &now), None, "{t:?}");
        }
    }

    #[test]
    fn timing_json_is_the_shape_the_mcp_tool_writes() {
        let cases = [
            (r#"{"kind":"interval","every_minutes":30}"#, Timing::Interval { every_minutes: 30 }),
            (r#"{"kind":"daily","time":"08:30"}"#, daily("08:30")),
            (
                r#"{"kind":"weekly","weekdays":[1,3],"time":"08:30"}"#,
                Timing::Weekly { weekdays: vec![1, 3], time: "08:30".into() },
            ),
            (r#"{"kind":"once","at":"2026-11-09T08:30"}"#, Timing::Once { at: "2026-11-09T08:30".into() }),
        ];
        for (json, timing) in cases {
            assert_eq!(serde_json::from_str::<Timing>(json).unwrap(), timing);
            assert_eq!(serde_json::to_string(&timing).unwrap(), json);
        }
    }

    pub async fn db() -> Db {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        sqlx::query("INSERT INTO projects (id, name, path, created_at) VALUES ('p1', 'proj', '/tmp/p1', 'c')")
            .execute(&db)
            .await
            .unwrap();
        db
    }

    pub fn input(id: Option<&str>, timing: Timing) -> ScheduleInput {
        ScheduleInput {
            id: id.map(str::to_string),
            project_id: "p1".into(),
            name: " Review PRs ".into(),
            prompt: "Review the open PRs".into(),
            engine: None,
            permission_mode: "plan".into(),
            timing,
        }
    }

    #[tokio::test]
    async fn a_new_schedule_starts_off_and_switching_on_sets_the_next_slot() {
        let db = db().await;
        let now = at("2026-11-08T08:00:00+07:00");
        let s = save(&db, input(None, daily("08:30")), false, &now).await.unwrap();
        assert_eq!(s.name, "Review PRs");
        assert!(!s.enabled && !s.proposed && s.next_run_at.is_none());

        let s = set_enabled(&db, &s.id, true, &now).await.unwrap();
        assert!(s.enabled);
        assert_eq!(s.next_run_at.as_deref(), Some("2026-11-08T01:30:00Z"));
        let s = set_enabled(&db, &s.id, false, &now).await.unwrap();
        assert!(!s.enabled && s.next_run_at.is_none());

        let past = save(&db, input(None, Timing::Once { at: "2026-11-01T08:00".into() }), false, &now).await.unwrap();
        assert!(set_enabled(&db, &past.id, true, &now).await.is_err());
        assert!(!get(&db, &past.id).await.unwrap().enabled);
    }

    #[tokio::test]
    async fn bad_input_is_refused() {
        let db = db().await;
        let now = at("2026-11-08T08:00:00+07:00");
        let mut cases = vec![input(None, daily("8:30"))];
        cases.push(ScheduleInput { name: "  ".into(), ..input(None, daily("08:30")) });
        cases.push(ScheduleInput { prompt: String::new(), ..input(None, daily("08:30")) });
        cases.push(ScheduleInput { engine: Some("gpt".into()), ..input(None, daily("08:30")) });
        cases.push(ScheduleInput { permission_mode: "auto".into(), ..input(None, daily("08:30")) });
        cases.push(ScheduleInput { project_id: "nope".into(), ..input(None, daily("08:30")) });
        for case in cases {
            assert!(save(&db, case.clone(), false, &now).await.is_err(), "{case:?}");
        }
        assert!(list(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_proposal_runs_only_after_it_is_saved() {
        let db = db().await;
        let now = at("2026-11-08T08:00:00+07:00");
        sqlx::query(
            "INSERT INTO schedules (id, project_id, name, prompt, timing, proposed, created_at, updated_at)
             VALUES ('s1', 'p1', 'n', 'p', '{\"kind\":\"daily\",\"time\":\"09:00\"}', 1, 'c', 'c')",
        )
        .execute(&db)
        .await
        .unwrap();
        assert!(set_enabled(&db, "s1", true, &now).await.is_err());
        let s = save(&db, input(Some("s1"), daily("09:00")), false, &now).await.unwrap();
        assert!(!s.proposed && !s.enabled, "saving reviews it, it stays off");
        assert!(set_enabled(&db, "s1", true, &now).await.unwrap().enabled);
    }

    #[tokio::test]
    async fn a_save_keeps_it_on_locally_and_turns_it_off_from_a_remote_desktop() {
        let db = db().await;
        let now = at("2026-11-08T08:00:00+07:00");
        let id = save(&db, input(None, daily("08:30")), false, &now).await.unwrap().id;
        set_enabled(&db, &id, true, &now).await.unwrap();

        let s = save(&db, input(Some(&id), daily("10:00")), false, &now).await.unwrap();
        assert!(s.enabled);
        assert_eq!(s.next_run_at.as_deref(), Some("2026-11-08T03:00:00Z"), "slot moved with the time");

        let s = save(&db, input(Some(&id), daily("10:00")), true, &now).await.unwrap();
        assert!(!s.enabled && s.next_run_at.is_none());
    }

    #[tokio::test]
    async fn schedules_go_with_their_project() {
        let db = db().await;
        let now = at("2026-11-08T08:00:00+07:00");
        save(&db, input(None, daily("08:30")), false, &now).await.unwrap();
        sqlx::query("DELETE FROM projects WHERE id = 'p1'").execute(&db).await.unwrap();
        assert!(list(&db).await.unwrap().is_empty());
    }
}
