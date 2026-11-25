// A session's automatic short name (src-tauri/src/runs/auto_title.rs): the
// backend replaces the app-set title once and announces it, so the sidebar and
// every cached run label follow without a reload. Pure, tested in
// __tests__/runTitle.test.mjs.

/** Global (not per run): the name can land after the run's own listeners stopped. */
export const EVT_RUN_TITLE = 'run:title'

export interface RunTitleEvent {
  run_id: string
  title: string
}

interface Titled {
  id: string
  title: string | null
}

/** Set `title` on the run's row in `runs` and in `runMeta`, wherever it is loaded. */
export function applyRunTitle(
  runs: Titled[],
  runMeta: Map<string, Titled>,
  runId: string,
  title: string | null,
): void {
  const run = runs.find(r => r.id === runId)
  if (run) run.title = title
  const cached = runMeta.get(runId)
  if (cached) cached.title = title
}
