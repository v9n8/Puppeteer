// Fork / duplicate a session (UX-28): which sessions offer it, what a refusal
// from `fork_run` (runs::fork) means, and the "Forked from …" line of a fork.
// Kept free of Vue / Tauri / `@/` imports so it can be unit-tested with `node --test`.

interface ForkableRun {
  id: string
  status: string
  engine: string
  title?: string | null
  role?: string | null
  conductor_run_id?: string | null
  forked_from_run_id?: string | null
}

/**
 * A stopped Claude / Codex session that has run. Not a Conductor worker: it
 * runs in its conductor's worktree, which goes with the conductor.
 */
export function canForkRun(run: ForkableRun): boolean {
  return (
    (run.engine === 'claude' || run.engine === 'codex') &&
    run.status !== 'running' &&
    run.status !== 'fetched' &&
    run.role !== 'worker' &&
    !run.conductor_run_id
  )
}

/** Refusals `fork_run` names, each with its own message (`run.fork.errors.*`). */
export const FORK_ERRORS = [
  'run_active',
  'run_trashed',
  'engine',
  'fork_worker',
  'not_started',
  'no_log',
  'fork_point_unknown',
] as const

/** The message key of a `fork_run` refusal; null for any other error (shown as is). */
export function forkErrorKey(error: unknown): string | null {
  const code = String(error)
  return (FORK_ERRORS as readonly string[]).includes(code) ? `run.fork.errors.${code}` : null
}

/**
 * Where a fork came from: the source session while it is in `runs`, 'gone'
 * when it is not (in the Trash, or older than the loaded list), null for a
 * session that is no fork.
 */
export function forkSource<R extends ForkableRun>(run: R | null | undefined, runs: readonly R[]): R | 'gone' | null {
  const from = run?.forked_from_run_id
  if (!from) return null
  return runs.find((r) => r.id === from) ?? 'gone'
}
