// Live data behind the Running screen's Session tab: the pure rows of
// lib/runningSessions built from the live-runs store and the run records, and
// the ways a row is opened. Opening reuses the dock's rule (useActiveRunRows).
import { computed } from 'vue'
import { useLiveRunsStore } from '@/stores/liveRuns'
import { useRunsStore } from '@/stores/runs'
import { useProjectsStore } from '@/stores/projects'
import { hasStuckAlert } from '@/lib/stuckNotify'
import { runningRows, type LiveLike, type RunningRow } from '@/lib/runningSessions'
import { openSessionWindow } from '@/lib/sessionWindow'
import { useActiveRunRows } from '@/composables/useActiveRunRows'

export function useRunningRows() {
  const live = useLiveRunsStore()
  const runsStore = useRunsStore()
  const projectsStore = useProjectsStore()
  const { open } = useActiveRunRows()

  const rows = computed<RunningRow[]>(() => {
    const sessions: LiveLike[] = []
    live.sessions.forEach((s) => sessions.push({ ...s, stuck: hasStuckAlert(s.attention) }))
    return runningRows(sessions, runsStore.runMeta, projectsStore.projects)
  })

  /** Open the Session in the main window and mark it seen, as the dock does. */
  function openRow(r: RunningRow) {
    open({
      runId: r.runId,
      projectId: r.projectId,
      status: r.status,
      pending: r.pending,
      done: r.unseen,
      role: r.isConductor ? 'conductor' : null,
      conductorRunId: null,
      endReason: r.endReason,
      stuck: r.stuck,
    })
  }

  async function openRowInWindow(r: RunningRow) {
    live.markSeen(r.runId)
    await openSessionWindow(r.projectId, r.runId)
  }

  return { rows, openRow, openRowInWindow }
}
