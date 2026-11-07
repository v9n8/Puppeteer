//! What runs after a feature switch is written: each background service's hook
//! starts or stops it at once (no restart), then every window is told.

use super::ModuleState;
use tauri::{AppHandle, Emitter};

/// Broadcast after every write, with the resolved tree.
pub const CHANGED_EVENT: &str = "features:changed";

/// The resolved tree before and after one write.
pub struct Change<'a> {
    pub before: &'a [ModuleState],
    pub after: &'a [ModuleState],
}

/// A service's reaction to a write. It runs on every write, so it compares
/// `before` and `after` itself, and spawns any async work.
pub type Hook = fn(&AppHandle, &Change<'_>);

/// One hook per background service (D0-09 … D0-17).
const HOOKS: &[Hook] = &[crate::inbox_api::server::on_features_changed];

/// Run the hooks, then emit [`CHANGED_EVENT`].
pub fn changed(app: &AppHandle, before: &[ModuleState], after: &[ModuleState]) {
    run(HOOKS, app, &Change { before, after });
    let _ = app.emit(CHANGED_EVENT, after);
}

fn run<A>(hooks: &[fn(&A, &Change<'_>)], app: &A, change: &Change<'_>) {
    for hook in hooks {
        hook(app, change);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::resolve;
    use std::collections::HashMap;
    use std::sync::Mutex;

    static SEEN: Mutex<Vec<(Vec<ModuleState>, Vec<ModuleState>)>> = Mutex::new(Vec::new());

    fn record(_: &(), change: &Change<'_>) {
        SEEN.lock().unwrap().push((change.before.to_vec(), change.after.to_vec()));
    }

    #[test]
    fn hooks_get_the_states_before_and_after() {
        let before = resolve(&HashMap::new());
        let after = resolve(&HashMap::from([("feature.remote".to_string(), "true".to_string())]));
        run(&[record, record], &(), &Change { before: &before, after: &after });
        let seen = SEEN.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert!(seen.iter().all(|(b, a)| *b == before && *a == after));
    }
}
