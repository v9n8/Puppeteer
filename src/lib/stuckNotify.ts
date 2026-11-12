// "Session may be stuck" (HD-11): which backend watchdog notices (`run:attention`)
// count as stuck, and which of them have not been notified yet. Only the two
// strong signals count: the sidecar stopped responding, or never started. The
// other notices (quiet turn, long tool, unmonitored) stay in-session banners.

/** The attention kinds that mean the sidecar itself may be dead (backend `liveness.rs`). */
export const STUCK_ALERT_KINDS = ['unresponsive', 'not_started'] as const

/** One `run:attention` notice; a missing `active` means it is shown. */
export interface AttentionNotice {
  key: string
  kind: string
  active?: boolean
}

export interface RunAttentionState {
  runId: string
  projectId: string
  attention: readonly AttentionNotice[]
}

export interface StuckAlert {
  runId: string
  projectId: string
  key: string
  kind: string
}

export function isStuckAlert(notice: AttentionNotice): boolean {
  return notice.active !== false && (STUCK_ALERT_KINDS as readonly string[]).includes(notice.kind)
}

/** Whether a run's notices include a stuck one (the dock's warning mark). */
export function hasStuckAlert(attention: readonly AttentionNotice[]): boolean {
  return attention.some(isStuckAlert)
}

/** De-duplication key of one alert: run id + notice key. */
export function stuckAlertKey(runId: string, key: string): string {
  return `${runId}\n${key}`
}

/**
 * The stuck alerts in `runs` not in `notified` yet, and the keys to remember
 * next time: every stuck alert still shown. An alert the backend cleared drops
 * out of the keys, so the same alert raised again later notifies again.
 */
export function newStuckAlerts(
  runs: readonly RunAttentionState[],
  notified: ReadonlySet<string>,
): { fresh: StuckAlert[]; notified: Set<string> } {
  const fresh: StuckAlert[] = []
  const next = new Set<string>()
  for (const run of runs) {
    for (const notice of run.attention) {
      if (!isStuckAlert(notice)) continue
      const id = stuckAlertKey(run.runId, notice.key)
      if (next.has(id)) continue
      next.add(id)
      if (!notified.has(id)) fresh.push({ runId: run.runId, projectId: run.projectId, key: notice.key, kind: notice.kind })
    }
  }
  return { fresh, notified: next }
}
