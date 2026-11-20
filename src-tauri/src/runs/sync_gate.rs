//! One mirror sync per transcript at a time (RT-29).
//!
//! The session watcher fires a sync for a transcript on every debounced change.
//! A big transcript still being appended to (an external CLI session) changes
//! again before the previous sync is done, and starting a new sync each time
//! ran several syncs of the same run side by side: each parses the whole
//! transcript and rewrites the same log and usage rows, and together they held
//! the DB pool long enough for other queries to time out (2026-11-16).
//!
//! [`SyncGate::run`] keeps one pass per key in flight. A request that comes in
//! while a pass runs only marks the key dirty; when the pass ends, exactly one
//! more pass runs to pick up what changed meanwhile, however many requests
//! came in.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::Mutex;

/// Per-key state: present while a pass for the key is running; `true` when
/// another request came in during that pass.
pub struct SyncGate<K> {
    slots: Mutex<HashMap<K, bool>>,
}

impl<K: Eq + Hash + Clone> Default for SyncGate<K> {
    fn default() -> Self {
        Self { slots: Mutex::new(HashMap::new()) }
    }
}

impl<K: Eq + Hash + Clone> SyncGate<K> {
    /// Run `pass` for `key` unless a pass for it is already running, in which
    /// case mark the key dirty and return at once (the running caller then
    /// runs one more pass). Passes for different keys run independently.
    pub async fn run<F, Fut>(&self, key: K, mut pass: F)
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = ()>,
    {
        if !self.enter(&key) {
            return;
        }
        // Frees the slot if a pass panics or the task is dropped, so later
        // changes to this file are not ignored for good.
        let mut slot = Slot { gate: self, key, held: true };
        loop {
            pass().await;
            if !self.again(&slot.key) {
                slot.held = false;
                break;
            }
        }
    }

    /// Take the key's slot (`true`), or mark the running pass dirty (`false`).
    fn enter(&self, key: &K) -> bool {
        let mut slots = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        match slots.get_mut(key) {
            Some(dirty) => {
                *dirty = true;
                false
            }
            None => {
                slots.insert(key.clone(), false);
                true
            }
        }
    }

    /// After a pass: `true` (slot kept, dirty cleared) when a request came in
    /// during it; otherwise release the slot and return `false`.
    fn again(&self, key: &K) -> bool {
        let mut slots = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        match slots.get_mut(key) {
            Some(dirty) if *dirty => {
                *dirty = false;
                true
            }
            _ => {
                slots.remove(key);
                false
            }
        }
    }
}

struct Slot<'a, K: Eq + Hash> {
    gate: &'a SyncGate<K>,
    key: K,
    held: bool,
}

impl<K: Eq + Hash> Drop for Slot<'_, K> {
    fn drop(&mut self) {
        if self.held {
            let mut slots = self.gate.slots.lock().unwrap_or_else(|e| e.into_inner());
            slots.remove(&self.key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::Notify;

    /// Counts passes and the most passes ever running at once; each pass waits
    /// for `release` before it ends.
    #[derive(Default)]
    struct Probe {
        passes: AtomicUsize,
        running: AtomicUsize,
        max_running: AtomicUsize,
        started: Notify,
        release: Notify,
    }

    impl Probe {
        async fn pass(&self) {
            let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_running.fetch_max(now, Ordering::SeqCst);
            self.passes.fetch_add(1, Ordering::SeqCst);
            self.started.notify_one();
            self.release.notified().await;
            self.running.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn requests_during_a_pass_run_exactly_one_more_pass_never_in_parallel() {
        let gate = Arc::new(SyncGate::<String>::default());
        let probe = Arc::new(Probe::default());
        let spawn = || {
            let (gate, probe) = (gate.clone(), probe.clone());
            tokio::spawn(async move { gate.run("a.jsonl".to_string(), || probe.pass()).await })
        };

        let first = spawn();
        probe.started.notified().await;
        // Five more changes while the first pass is still running: none starts
        // a pass of its own, each returns at once.
        for _ in 0..5 {
            spawn().await.unwrap();
        }
        assert_eq!(probe.passes.load(Ordering::SeqCst), 1);

        probe.release.notify_one();
        probe.started.notified().await; // the one catch-up pass
        assert_eq!(probe.passes.load(Ordering::SeqCst), 2);
        probe.release.notify_one();
        first.await.unwrap();

        assert_eq!(probe.passes.load(Ordering::SeqCst), 2, "one catch-up pass for the whole burst");
        assert_eq!(probe.max_running.load(Ordering::SeqCst), 1, "passes of one file never overlap");

        // The slot is free again: the next change runs a pass straight away.
        let next = spawn();
        probe.started.notified().await;
        probe.release.notify_one();
        next.await.unwrap();
        assert_eq!(probe.passes.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn a_quiet_pass_does_not_rerun_and_other_files_are_not_held_up() {
        let gate = SyncGate::<&str>::default();
        let runs = AtomicUsize::new(0);
        gate.run("a", || async { runs.fetch_add(1, Ordering::SeqCst); }).await;
        assert_eq!(runs.load(Ordering::SeqCst), 1);

        // While "a" is mid-pass, "b" still gets its own pass.
        let inner = AtomicUsize::new(0);
        gate.run("a", || async {
            gate.run("b", || async { inner.fetch_add(1, Ordering::SeqCst); }).await;
        })
        .await;
        assert_eq!(inner.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_panicking_pass_frees_the_slot() {
        let gate = Arc::new(SyncGate::<&'static str>::default());
        let g = gate.clone();
        let failed = tokio::spawn(async move { g.run("a", || async { panic!("boom") }).await }).await;
        assert!(failed.is_err());
        let runs = AtomicUsize::new(0);
        gate.run("a", || async { runs.fetch_add(1, Ordering::SeqCst); }).await;
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }
}
