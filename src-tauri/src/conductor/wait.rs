//! When a `session_wait` call returns (CO-01). Shared by the streaming path
//! (`mcp_http::session_wait_stream`) and the blocking fallback
//! (`tools::session_wait`) so the two cannot drift apart.
//!
//! By default the call returns as soon as ONE worker finishes: waiting for the
//! slowest worker left the conductor blind to a finished one for minutes, unable
//! to read its result and hand out the next piece of work.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReturnOn {
    /// A worker that was running when the call began has finished.
    Any,
    /// Every worker is idle (the behaviour before CO-01).
    All,
}

/// The call's `return_on`: `"any"` (also when absent) or `"all"`.
pub(crate) fn return_on(args: &Value) -> Result<ReturnOn, String> {
    match args.get("return_on") {
        None | Some(Value::Null) => Ok(ReturnOn::Any),
        Some(v) => match v.as_str() {
            Some("any") => Ok(ReturnOn::Any),
            Some("all") => Ok(ReturnOn::All),
            _ => Err("`return_on` must be \"any\" or \"all\"".to_string()),
        },
    }
}

/// Why a wait ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    Settled,
    /// The ceiling was reached first; the result says `timed_out: true`.
    TimedOut,
}

/// Whether the wait ends on this snapshot. `running_at_start` is the `pending`
/// list of the call's first snapshot, `pending` the current one, `expired`
/// whether the ceiling has passed. A settled wait wins over an expired one, so
/// `timed_out` is only set when the call returns for lack of time.
pub(crate) fn verdict(
    return_on: ReturnOn,
    running_at_start: &[String],
    pending: &[String],
    expired: bool,
) -> Option<End> {
    let settled = match return_on {
        ReturnOn::All => pending.is_empty(),
        // Only a worker running when the call began counts. One that was idle
        // before would end every call at once, and a conductor waiting on a
        // group that has one would loop on `session_wait`.
        ReturnOn::Any => {
            running_at_start.is_empty() || running_at_start.iter().any(|id| !pending.contains(id))
        }
    };
    if settled {
        Some(End::Settled)
    } else if expired {
        Some(End::TimedOut)
    } else {
        None
    }
}

/// One wait in progress: remembers which workers were running at its first
/// snapshot and judges each snapshot with [`verdict`].
pub(crate) struct Watch {
    return_on: ReturnOn,
    running_at_start: Option<Vec<String>>,
}

impl Watch {
    pub(crate) fn new(return_on: ReturnOn) -> Self {
        Self { return_on, running_at_start: None }
    }

    pub(crate) fn check(&mut self, pending: &[String], expired: bool) -> Option<End> {
        let start = self.running_at_start.get_or_insert_with(|| pending.to_vec());
        verdict(self.return_on, start, pending, expired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn any_returns_when_one_worker_finishes() {
        let start = ids(&["a", "b"]);
        assert_eq!(verdict(ReturnOn::Any, &start, &ids(&["a", "b"]), false), None);
        assert_eq!(verdict(ReturnOn::Any, &start, &ids(&["b"]), false), Some(End::Settled));
    }

    #[test]
    fn any_ignores_a_worker_idle_before_the_call() {
        // `c` was idle at the start: it is in `done`, but not a reason to return.
        let mut w = Watch::new(ReturnOn::Any);
        assert_eq!(w.check(&ids(&["a", "b"]), false), None);
        assert_eq!(w.check(&ids(&["a", "b"]), false), None);
        assert_eq!(w.check(&ids(&["a"]), false), Some(End::Settled));
    }

    #[test]
    fn any_returns_at_once_when_nothing_is_running() {
        let mut w = Watch::new(ReturnOn::Any);
        assert_eq!(w.check(&[], false), Some(End::Settled));
        assert_eq!(verdict(ReturnOn::Any, &[], &[], false), Some(End::Settled));
    }

    #[test]
    fn all_waits_for_every_worker() {
        let mut w = Watch::new(ReturnOn::All);
        assert_eq!(w.check(&ids(&["a", "b"]), false), None);
        assert_eq!(w.check(&ids(&["b"]), false), None);
        assert_eq!(w.check(&[], false), Some(End::Settled));
        let mut idle = Watch::new(ReturnOn::All);
        assert_eq!(idle.check(&[], false), Some(End::Settled));
    }

    #[test]
    fn the_ceiling_ends_the_wait_as_timed_out_unless_it_settled() {
        let start = ids(&["a", "b"]);
        for on in [ReturnOn::Any, ReturnOn::All] {
            assert_eq!(verdict(on, &start, &start, true), Some(End::TimedOut), "{on:?}");
        }
        assert_eq!(verdict(ReturnOn::All, &start, &ids(&["b"]), true), Some(End::TimedOut));
        // Settled on the same snapshot the ceiling passed: not a timeout.
        assert_eq!(verdict(ReturnOn::Any, &start, &ids(&["b"]), true), Some(End::Settled));
        assert_eq!(verdict(ReturnOn::All, &start, &[], true), Some(End::Settled));
    }

    #[test]
    fn return_on_defaults_to_any_and_rejects_other_values() {
        assert_eq!(return_on(&json!({})), Ok(ReturnOn::Any));
        assert_eq!(return_on(&json!({ "return_on": null })), Ok(ReturnOn::Any));
        assert_eq!(return_on(&json!({ "return_on": "any" })), Ok(ReturnOn::Any));
        assert_eq!(return_on(&json!({ "return_on": "all" })), Ok(ReturnOn::All));
        assert!(return_on(&json!({ "return_on": "first" })).is_err());
        assert!(return_on(&json!({ "return_on": 1 })).is_err());
    }
}
