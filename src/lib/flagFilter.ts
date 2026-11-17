// The History list's "Filter by flag" (V3-19): pure helpers, so the numbers in the
// menu, the filter and the "History · N" count all come from the same rules.
import type { SessionFlag } from './sessionFlags.ts'

/** `all`: every session · `none`: sessions without a flag · else a flag id. */
export type FlagFilter = 'all' | 'none' | string

interface FlaggedRun {
  flag_id?: string | null
  conductor_run_id?: string | null
}

export interface FlagCounts {
  /** Sessions the History list shows (workers left out, like the list). */
  total: number
  none: number
  byFlag: Map<string, number>
}

/** The runs that pass `filter` (all of them for `all`). */
export function filterByFlag<R extends FlaggedRun>(runs: readonly R[], filter: FlagFilter): R[] {
  if (filter === 'all') return [...runs]
  if (filter === 'none') return runs.filter((r) => !r.flag_id)
  return runs.filter((r) => r.flag_id === filter)
}

/**
 * Clears (in place) the `flag_id` of runs whose flag was deleted elsewhere. `flagsOf` gives
 * the flags a project's sessions see, or null while that list is not loaded (those runs stay
 * as they are). Returns how many runs changed.
 */
export function clearGoneFlags(
  runs: Iterable<{ project_id: string; flag_id?: string | null }>,
  flagsOf: (projectId: string) => readonly { id: string }[] | null,
): number {
  let cleared = 0
  for (const run of runs) {
    if (!run.flag_id) continue
    const flags = flagsOf(run.project_id)
    if (flags && !flags.some((f) => f.id === run.flag_id)) {
      run.flag_id = null
      cleared += 1
    }
  }
  return cleared
}

/** Sessions per flag over what History lists: workers never get a row, so they are not counted. */
export function flagCounts(runs: readonly FlaggedRun[]): FlagCounts {
  const counts: FlagCounts = { total: 0, none: 0, byFlag: new Map() }
  for (const run of runs) {
    if (run.conductor_run_id) continue
    counts.total += 1
    if (run.flag_id) counts.byFlag.set(run.flag_id, (counts.byFlag.get(run.flag_id) ?? 0) + 1)
    else counts.none += 1
  }
  return counts
}

export interface FlagOption<F> {
  flag: F
  count: number
}

type OptionFlag = Pick<SessionFlag, 'id' | 'project_id' | 'hidden'>

/**
 * The rows of the filter menu: shared flags then the project's own, each with its
 * count. A flag the project hides is listed only while sessions still carry it, so
 * those stay reachable.
 */
export function filterOptions<F extends OptionFlag>(
  flags: readonly F[],
  counts: FlagCounts,
): { shared: FlagOption<F>[]; project: FlagOption<F>[] } {
  const options = flags
    .map((flag) => ({ flag, count: counts.byFlag.get(flag.id) ?? 0 }))
    .filter((o) => !o.flag.hidden || o.count > 0)
  return {
    shared: options.filter((o) => o.flag.project_id === null),
    project: options.filter((o) => o.flag.project_id !== null),
  }
}

/** `all` when the filter names a flag that is no longer listed (deleted elsewhere), else the filter. */
export function resetIfGone(filter: FlagFilter, flags: readonly Pick<SessionFlag, 'id'>[]): FlagFilter {
  if (filter === 'all' || filter === 'none') return filter
  return flags.some((f) => f.id === filter) ? filter : 'all'
}
