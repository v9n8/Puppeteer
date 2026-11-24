//! Background tasks a Claude session keeps alive after its turn (BG-01): a
//! backgrounded Bash, a Monitor, a background subagent. While one lives the
//! sidecar stays up (the CLI wakes the session when it ends, see
//! `sidecar::result_should_close_stdin`), so `runs.status` stays `running`
//! although the turn is over. This tells the two apart for the webview and the
//! conductor: which tasks are alive, and whether the last turn has ended.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use super::{RunHandles, RunRegistry};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BgTask {
    pub task_id: String,
    pub task_type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_type: Option<String>,
}

/// The tasks of a `background_tasks_changed` event: its `tasks` array is the
/// whole live set (replace, never merge). `ambient` ones are left out, as the
/// drain's count leaves them out: they watch passively and never end.
pub fn blocking_tasks(tasks: &Value) -> Vec<BgTask> {
    let text = |task: &Value, key: &str| task.get(key).and_then(Value::as_str).map(str::to_string);
    tasks.as_array().map_or_else(Vec::new, |all| {
        all.iter()
            .filter(|task| task.get("ambient").and_then(Value::as_bool) != Some(true))
            .filter_map(|task| {
                Some(BgTask {
                    task_id: text(task, "task_id")?,
                    task_type: text(task, "task_type").unwrap_or_default(),
                    description: text(task, "description").unwrap_or_default(),
                    subagent_type: text(task, "subagent_type"),
                })
            })
            .collect()
    })
}

/// What `run:bg_tasks:{run_id}` and `get_background_tasks` carry.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BgSnapshot {
    pub run_id: String,
    /// The session's last turn has ended and the live session takes a new
    /// prompt at once: a follow-up starts a turn now instead of waiting.
    pub turn_done: bool,
    pub tasks: Vec<BgTask>,
    /// When that turn ended (RFC 3339, like `runs.finished_at`).
    pub turn_ended_at: Option<String>,
}

/// Over, not stopping, and its input still open: a session whose input is
/// closed is on its way out, and a follow-up must wait for its exit.
fn turn_done(busy: bool, stopping: bool, input_open: bool, turn_ended_at: Option<&str>) -> bool {
    !busy && !stopping && input_open && turn_ended_at.is_some()
}

pub fn snapshot(run_id: &str, handles: &RunHandles) -> BgSnapshot {
    let gate = &handles.turn_gate;
    BgSnapshot {
        run_id: run_id.to_string(),
        turn_done: turn_done(gate.busy, gate.stopping, handles.stdin.is_some(), gate.turn_ended_at.as_deref()),
        tasks: gate.bg_tasks.clone(),
        turn_ended_at: gate.turn_ended_at.clone(),
    }
}

/// Tell the webview when the run's snapshot changed since `last`, which the
/// drain keeps between calls.
pub async fn publish(app: &AppHandle, registry: &RunRegistry, run_id: &str, last: &mut Option<BgSnapshot>) {
    let Some(snap) = registry.lock().await.get(run_id).map(|h| snapshot(run_id, h)) else {
        return;
    };
    if last.as_ref() == Some(&snap) {
        return;
    }
    let _ = app.emit(&format!("run:bg_tasks:{run_id}"), &snap);
    *last = Some(snap);
}

/// The snapshot of a live run, for a webview that (re)loaded after the event
/// went out; None when the run is not live.
#[tauri::command]
pub async fn get_background_tasks(
    registry: State<'_, RunRegistry>,
    run_id: String,
) -> Result<Option<BgSnapshot>, String> {
    Ok(registry.lock().await.get(&run_id).map(|h| snapshot(&run_id, h)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ambient_tasks_are_left_out() {
        let tasks = json!([
            { "task_id": "a", "task_type": "local_bash", "description": "grep foo" },
            { "task_id": "w", "task_type": "monitor", "description": "artifact watch", "ambient": true },
            { "task_id": "s", "task_type": "local_agent", "description": "review", "subagent_type": "general-purpose", "ambient": false },
        ]);
        let got = blocking_tasks(&tasks);
        assert_eq!(got.iter().map(|t| t.task_id.as_str()).collect::<Vec<_>>(), ["a", "s"]);
        assert_eq!(got[1].subagent_type.as_deref(), Some("general-purpose"));
        assert_eq!(got[0].subagent_type, None);
    }

    #[test]
    fn each_event_replaces_the_set() {
        let first = blocking_tasks(&json!([
            { "task_id": "a", "task_type": "local_bash", "description": "one" },
            { "task_id": "b", "task_type": "local_bash", "description": "two" },
        ]));
        assert_eq!(first.len(), 2);
        // The next event names only what is still alive.
        let second = blocking_tasks(&json!([{ "task_id": "b", "task_type": "local_bash", "description": "two" }]));
        assert_eq!(second.iter().map(|t| t.task_id.as_str()).collect::<Vec<_>>(), ["b"]);
        assert!(blocking_tasks(&json!([])).is_empty());
        assert!(blocking_tasks(&Value::Null).is_empty());
    }

    #[test]
    fn a_task_without_an_id_cannot_be_stopped_so_it_is_skipped() {
        let got = blocking_tasks(&json!([{ "task_type": "local_bash", "description": "?" }, { "task_id": "x" }]));
        assert_eq!(got, [BgTask { task_id: "x".into(), task_type: String::new(), description: String::new(), subagent_type: None }]);
    }

    #[test]
    fn the_turn_is_done_only_when_the_live_session_waits_for_input() {
        let at = Some("2026-11-24T10:59:33+00:00");
        assert!(turn_done(false, false, true, at));
        // A turn runs (a follow-up, or a wake-up the CLI started itself).
        assert!(!turn_done(true, false, true, at));
        // A Stop waits for the turn to end.
        assert!(!turn_done(false, true, true, at));
        // The input is closed: the session is ending, a follow-up waits for its exit.
        assert!(!turn_done(false, false, false, at));
        // No turn of this process has ended yet (a resume before its first prompt).
        assert!(!turn_done(false, false, true, None));
    }

    #[test]
    fn the_snapshot_serializes_as_the_webview_reads_it() {
        let snap = BgSnapshot {
            run_id: "r1".into(),
            turn_done: true,
            tasks: vec![BgTask { task_id: "a".into(), task_type: "local_bash".into(), description: "grep".into(), subagent_type: None }],
            turn_ended_at: Some("t".into()),
        };
        assert_eq!(
            serde_json::to_value(&snap).unwrap(),
            json!({ "run_id": "r1", "turn_done": true, "turn_ended_at": "t",
                    "tasks": [{ "task_id": "a", "task_type": "local_bash", "description": "grep" }] })
        );
    }
}
