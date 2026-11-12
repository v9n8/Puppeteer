/**
 * Sessions cut off by the app (HD-04 / HD-05): the banner's rows, its picker
 * selection, what a resume left behind, and the History label of a run the
 * user did not stop. Nothing here resumes anything: only the banner's buttons
 * call `resume_interrupted_runs`.
 */

/** One banner row, as `list_interrupted_runs` returns it. */
export interface InterruptedRun {
  run_id: string
  project_id: string
  project_name: string
  title: string | null
  engine: string
  /** Workers of this run cut off with it; they never get a row of their own. */
  worker_count: number
  /** The permission mode a resume from the banner runs in. */
  permission_mode: string
}

/** What `resume_interrupted_runs` did for one row. */
export interface ResumeOutcome {
  run_id: string
  ok: boolean
  error?: string
}

/** The banner after a resume: the rows still waiting and each one's last error. */
export interface BannerState {
  rows: InterruptedRun[]
  errors: Record<string, string>
}

export function rowIds(rows: readonly InterruptedRun[]): string[] {
  return rows.map((r) => r.run_id)
}

/**
 * Tick or untick `id` in the picker. The selection keeps the banner's order, so
 * the runs resume in the order they are listed; an id no longer on the banner
 * drops out.
 */
export function toggleSelected(rows: readonly InterruptedRun[], selected: readonly string[], id: string): string[] {
  const next = new Set(selected)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  return rowIds(rows).filter((r) => next.has(r))
}

/**
 * Fold a resume's outcomes into the banner: a resumed row leaves, a failed one
 * stays with its own error, a row that was not asked for is left as it was.
 */
export function applyOutcomes(state: BannerState, outcomes: readonly ResumeOutcome[]): BannerState {
  const errors = { ...state.errors }
  const resumed = new Set<string>()
  for (const o of outcomes) {
    if (o.ok) {
      resumed.add(o.run_id)
      delete errors[o.run_id]
    } else {
      errors[o.run_id] = o.error ?? ''
    }
  }
  return { rows: state.rows.filter((r) => !resumed.has(r.run_id)), errors }
}

/**
 * i18n key of a History row's status. A run cut off by the app or stopped at
 * the quota limit is `cancelled` like one the user stopped; its `end_reason`
 * tells them apart.
 */
export function runStatusKey(run: { status: string; run_type: string; end_reason?: string | null }): string {
  if (run.status === 'fetched' && run.run_type === 'session') return 'common.status.draft'
  if (run.status === 'cancelled' && run.end_reason === 'app_closed') return 'common.status.appClosed'
  if (run.status === 'cancelled' && run.end_reason === 'quota') return 'common.status.quotaStopped'
  return `common.status.${run.status}`
}
