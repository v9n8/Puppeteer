// Conductor worker sidebar state for the run on screen: the worker tree, the
// panel's open/rail choice and the worker-cap editor. Part of the RunView split
// (see useRunSession.ts).
import { ref, computed, watch } from 'vue'
import type { RunSessionCore } from './useRunSessionCore'

export function useRunConductor(core: RunSessionCore) {
  const {
    t, runsStore, live, conductorStore, features, toast, projectId, currentRunId, currentStatus,
  } = core

  // ── Conductor ──────────────────────────────────────────────────────────────
  // A conductor is just this session that also drives worker runs. When the open
  // run is one, we show the worker sidebar. The conductor store tracks the worker
  // tree event-driven (membership + timeline); per-worker *status* comes live from
  // the `liveRuns` store, so a worker flipping running→done updates instantly
  // without any backend round-trip.
  const isConductor = computed(() => !!conductorStore.detail?.session)
  const conductorWorkers = computed(() =>
    (conductorStore.detail?.workers ?? []).map((w) => ({
      ...w,
      // Prefer the live status (from run:activated/run:done events); fall back to
      // the DB snapshot for workers liveRuns isn't tracking (e.g. already finished).
      status: live.get(w.worker_id)?.status ?? w.status,
    })),
  )
  // The conductor's concurrent-worker cap (set at launch, editable live from the
  // sidebar header). Shown next to the live worker count so the user sees headroom
  // at a glance. (Distinct from `conductorMaxWorkers`, the launch default applied
  // when a fresh session sends its first message.)
  const conductorWorkerCap = computed(() => conductorStore.detail?.session?.max_workers ?? 0)
  const conductorEvents = computed(() => conductorStore.detail?.events ?? [])
  // The conductor the worker panel is anchored to for the open run: the run itself
  // when it's a conductor, otherwise its parent conductor when the open run is a
  // worker (so viewing a worker in-place keeps the panel + its sibling list alive).
  // Falls back to the run's own id for ordinary sessions (store self-clears those).
  const conductorAnchorId = computed<string | null>(() => {
    const id = currentRunId.value
    if (!id) return null
    const meta = runsStore.runMeta.get(id) ?? runsStore.runs.find((r) => r.id === id)
    return meta?.conductor_run_id ?? id
  })
  // ── Worker panel open/rail ──────────────────────────────────────────────────
  // Auto: a session with no workers sits as the thin rail; it opens by itself
  // once a worker appears. A manual toggle is remembered per session (anchor id)
  // for this app run and then always wins — it never auto-flips back.
  // The worker count also reads the run list (stable across session switches),
  // so the panel doesn't flash to the rail while the new detail is loading.
  const workerPanelChoice = ref(new Map<string, boolean>())
  const anchorWorkerCount = computed(() => {
    const id = conductorAnchorId.value
    const fromRuns = id ? runsStore.runs.filter((r) => r.conductor_run_id === id).length : 0
    return Math.max(conductorWorkers.value.length, fromRuns)
  })
  const workerPanelOpen = computed(() => {
    const id = conductorAnchorId.value
    const manual = id ? workerPanelChoice.value.get(id) : undefined
    return manual ?? anchorWorkerCount.value > 0
  })
  function setWorkerPanelOpen(open: boolean) {
    const id = conductorAnchorId.value
    if (id) workerPanelChoice.value.set(id, open)
  }

  // Conductor ids that currently have at least one worker streaming. A conductor's
  // own status can be 'done' (it finished planning) while its workers keep running,
  // so we surface the running state on the parent row too — its icon stays the live
  // spinner until every worker settles. Worker status comes live from liveRuns,
  // falling back to the DB snapshot for workers liveRuns isn't tracking.
  const conductorIdsWithRunningWorker = computed(() => {
    const ids = new Set<string>()
    for (const r of runsStore.runs) {
      if (!r.conductor_run_id) continue
      const status = live.get(r.id)?.status ?? r.status
      if (status === 'running') ids.add(r.conductor_run_id)
    }
    return ids
  })

  // ── Worker-cap editing (sidebar header) ─────────────────────────────────────
  // Two states share one editor:
  //  • pre-launch (a fresh session / issue / PR not yet started) — the cap edits the
  //    local `conductorMaxWorkers` ref that seeds the launch; no backend row exists.
  //  • running/finished conductor — the cap is raised/lowered live: the backend
  //    clamps to [1,100], persists it, and the next session_spawn enforces it.
  // The draft is a separate ref so the 2s detail poll can't clobber it mid-edit.
  // Emitted value from <Input> is a string, so we parse on commit.
  const editingWorkerCap = ref(false)
  const workerCapDraft = ref<string | number>(0)
  const savingWorkerCap = ref(false)
  function beginEditWorkerCap() {
    workerCapDraft.value = effectiveWorkerCap.value || 1
    editingWorkerCap.value = true
  }
  // The rail's cap pill: expand the panel (a manual open) and edit the cap there.
  function editWorkerCapFromRail() {
    setWorkerPanelOpen(true)
    beginEditWorkerCap()
  }
  function cancelEditWorkerCap() {
    editingWorkerCap.value = false
  }
  async function commitWorkerCap() {
    if (!editingWorkerCap.value) return
    editingWorkerCap.value = false
    const parsed = Math.round(Number(workerCapDraft.value))
    if (!Number.isFinite(parsed)) return
    const next = Math.min(100, Math.max(1, parsed))
    // Pre-launch conductor: no session row yet, so just remember the launch cap.
    if (!isConductor.value) {
      conductorMaxWorkers.value = next
      return
    }
    // Target the anchored conductor — the open run may be one of its workers.
    const id = conductorStore.selectedId
    if (!id || next === conductorWorkerCap.value) return
    savingWorkerCap.value = true
    try {
      await conductorStore.setMaxWorkers(id, next)
    } catch (e) {
      toast.error(t('conductor.setMaxWorkersFailed', { error: String(e) }))
    } finally {
      savingWorkerCap.value = false
    }
  }

  // Every session is a conductor: any fetched (never-started) run — a fresh
  // session or a fetched issue/PR — launches through conductorStore.start on its
  // first message. `conductorMaxWorkers` is the launch-time cap, editable from the
  // worker sidebar before the goal is sent.
  const conductorMaxWorkers = ref(10)

  // The OPEN run is a pre-launch conductor. We surface the worker sidebar in this
  // state too so the user can set the worker limit BEFORE sending the goal.
  const pendingConductor = computed(
    () => !!currentRunId.value && !isConductor.value && currentStatus.value === 'fetched',
  )
  // Show the worker sidebar for a live conductor OR a pending (pre-launch) one —
  // not while Conductor is off: a launch then runs as a plain session.
  const showWorkerPanel = computed(
    () => features.allows('run.conductorPanel') && (isConductor.value || pendingConductor.value),
  )
  // Cap shown/edited in the header: the persisted live cap once launched, else the
  // local launch-time ref.
  const effectiveWorkerCap = computed(() =>
    isConductor.value ? conductorWorkerCap.value : conductorMaxWorkers.value,
  )

  // Point the conductor store at the conductor anchoring the open run (itself if it
  // IS a conductor, else its parent conductor when viewing a worker in-place); it
  // self-clears when the run is a normal session. Watching the anchor — not the raw
  // run id — means hopping between a conductor's workers keeps the same panel.
  watch(conductorAnchorId, (id) => {
    void conductorStore.select(id ?? null)
  }, { immediate: true })
  // Keep worker permission prompts flowing: listen to each running worker so its
  // queue populates (and the sidebar's ⚠ flag lights up) even before it's opened.
  watch(conductorWorkers, (ws) => {
    for (const w of ws) {
      if (w.status === 'running') {
        void live.startListening(w.worker_id, projectId.value)
      }
    }
  }, { immediate: true })

  return {
    conductorWorkers, conductorEvents, workerPanelOpen, setWorkerPanelOpen,
    conductorIdsWithRunningWorker, editingWorkerCap, workerCapDraft, savingWorkerCap,
    beginEditWorkerCap, editWorkerCapFromRail, cancelEditWorkerCap, commitWorkerCap,
    conductorMaxWorkers, pendingConductor, showWorkerPanel, effectiveWorkerCap,
  }
}

export type RunConductor = ReturnType<typeof useRunConductor>
