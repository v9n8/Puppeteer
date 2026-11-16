import { watch } from 'vue'
import { isRemoteMode } from '@/lib/ipc/remote'
import { saveRunStatuses } from '@/lib/windowRegistry'
import { runStatusOf, type WindowRunStatus } from '@/lib/windowsOverlay'
import { useLiveRunsStore } from '@/stores/liveRuns'

/**
 * Main window only: keeps the badge of every live run (running / waiting for the
 * user / error) in localStorage, where the "Open windows" overlay of any window
 * reads it — a session's own window knows its run, not the others'. A run with no
 * badge is left out, so the overlay never guesses.
 */
export function useWindowRunStatus(): void {
  if (isRemoteMode()) return
  const live = useLiveRunsStore()
  watch(
    () => {
      const statuses: Record<string, WindowRunStatus> = {}
      live.sessions.forEach((s) => {
        const status = runStatusOf({ status: s.status, pending: s.permissionQueue.length })
        if (status) statuses[s.runId] = status
      })
      return JSON.stringify(statuses)
    },
    (json) => saveRunStatuses(JSON.parse(json) as Record<string, WindowRunStatus>),
    { immediate: true },
  )
}
