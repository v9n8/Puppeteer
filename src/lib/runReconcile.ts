// Runs the UI still shows as running after their `run:done` was missed: the run
// ended while its listener was still attaching, a start/resume call failed after
// the UI had marked it running, or a webview reload dropped the liveRuns store
// mid-turn. Pure helpers behind `liveRuns.reconcileRunning` and the main
// window's start-up re-attach (`useMainWindowEvents`).

/**
 * The two-pass rule: a run is settled only once two consecutive passes miss it
 * in `live`. The UI flips a run to running before `start_run` / `resume_run`
 * registers its sidecar, so one pass of grace leaves a starting run alone.
 * Updates `stale` (the ids missed once) in place; returns the ids to settle now.
 */
export function settleAfterTwoPasses(candidates: string[], live: Set<string>, stale: Set<string>): string[] {
  for (const id of [...stale]) {
    if (!candidates.includes(id) || live.has(id)) stale.delete(id)
  }
  const settle: string[] = []
  for (const id of candidates) {
    if (live.has(id)) continue
    if (!stale.has(id)) {
      stale.add(id)
      continue
    }
    stale.delete(id)
    settle.push(id)
  }
  return settle
}

/**
 * Rows of the run list still marked running that no live session tracks. A
 * session settles its own row (`run:done`, or the reconcile of the session), so
 * only these need their status read back from the DB.
 */
export function untrackedRunningRows(
  rows: readonly { id: string; status: string }[],
  sessions: { has(id: string): boolean },
): string[] {
  return rows.filter((r) => r.status === 'running' && !sessions.has(r.id)).map((r) => r.id)
}

/**
 * Learn the project of every run in `runIds`: `list_live_runs` names runs only,
 * and no command reads one run by id. `load(projectId)` folds that project's runs
 * into what `projectOf` reads; projects are loaded one by one and the scan stops
 * as soon as every run is known. Returns the runs whose project was found.
 */
export async function resolveRunProjects(
  runIds: string[],
  projectOf: (runId: string) => string | undefined,
  projectIds: string[],
  load: (projectId: string) => Promise<void>,
): Promise<{ runId: string; projectId: string }[]> {
  for (const p of projectIds) {
    if (runIds.every((id) => projectOf(id))) break
    await load(p)
  }
  return runIds.flatMap((runId) => {
    const projectId = projectOf(runId)
    return projectId ? [{ runId, projectId }] : []
  })
}
