// Trash screen (TrashView): the shapes of `list_trash` (src-tauri/src/commands/trash.rs)
// and the grouping / sorting of its rows, as pure functions (tested in
// __tests__/trash.test.mjs).
//
// A run is in the Trash when it or its project is trashed. Trashing a project
// does not stamp its runs, so such a run has `deleted_at: null` and takes the
// project's time.

export const CMD_LIST_TRASH = 'list_trash'
/** Emitted by the backend after every move to / out of the Trash and every purge. */
export const EVT_TRASH_CHANGED = 'trash:changed'

export interface TrashProject {
  id: string
  name: string
  path: string
  created_at: string
  /** `null`: a live project, listed for its trashed sessions. */
  deleted_at: string | null
}

export interface TrashRun {
  id: string
  project_id: string
  title: string | null
  run_type: string
  ref_number: number | null
  status: string
  engine: string
  model: string | null
  created_at: string
  /** `null`: in the Trash only through its project. */
  deleted_at: string | null
  conductor_run_id: string | null
  /** Workers a purge of this run deletes with it (in the Trash or not). */
  worker_count: number
}

export interface TrashListing {
  projects: TrashProject[]
  runs: TrashRun[]
}

export type TrashSort = 'deleted' | 'created'
export const TRASH_SORTS: readonly TrashSort[] = ['deleted', 'created']

export interface TrashSession {
  run: TrashRun
  /** When it went to the Trash: its own stamp, else its project's. */
  deletedAt: string
  /** Workers in the Trash shown under this row instead of as rows of their own. */
  workers: TrashRun[]
}

export interface TrashGroup {
  project: TrashProject
  /** The project itself is in the Trash: the group is restored / purged as a project. */
  projectTrashed: boolean
  sessions: TrashSession[]
  /** Sessions a purge of the whole group deletes. */
  purgeCount: number
}

function time(iso: string | null | undefined): number {
  const ms = iso ? Date.parse(iso) : NaN
  return Number.isNaN(ms) ? 0 : ms
}

/** Sessions a purge of this row deletes: the run and every worker it spawned. */
export function sessionsLost(run: TrashRun): number {
  return 1 + run.worker_count
}

function sessionKey(s: TrashSession, sort: TrashSort): number {
  return time(sort === 'deleted' ? s.deletedAt : s.run.created_at)
}

/**
 * One group per project, newest first by `sort`; inside a group the sessions
 * are sorted the same way. A worker whose conductor is in the Trash too is
 * listed under it, since restoring or purging the conductor takes it along.
 */
export function groupTrash(listing: TrashListing, sort: TrashSort): TrashGroup[] {
  const inTrash = new Set(listing.runs.map((r) => r.id))
  const byProject = new Map<string, TrashRun[]>()
  for (const run of listing.runs) {
    const list = byProject.get(run.project_id)
    if (list) list.push(run)
    else byProject.set(run.project_id, [run])
  }

  const groups = listing.projects.map((project) => {
    const runs = byProject.get(project.id) ?? []
    const nested = (r: TrashRun) =>
      r.conductor_run_id !== null && r.conductor_run_id !== r.id && inTrash.has(r.conductor_run_id)
    const sessions: TrashSession[] = runs
      .filter((r) => !nested(r))
      .map((run) => ({
        run,
        deletedAt: run.deleted_at ?? project.deleted_at ?? '',
        workers: runs.filter((w) => nested(w) && w.conductor_run_id === run.id),
      }))
      .sort((a, b) => sessionKey(b, sort) - sessionKey(a, sort) || a.run.id.localeCompare(b.run.id))
    const projectTrashed = project.deleted_at !== null
    return {
      project,
      projectTrashed,
      sessions,
      // A purged project takes every run with it; otherwise each row goes with its workers.
      purgeCount: projectTrashed ? runs.length : sessions.reduce((n, s) => n + sessionsLost(s.run), 0),
    }
  })

  const groupKey = (g: TrashGroup) =>
    g.sessions.length > 0
      ? sessionKey(g.sessions[0], sort)
      : time(sort === 'deleted' ? g.project.deleted_at : g.project.created_at)
  return groups.sort(
    (a, b) => groupKey(b) - groupKey(a) || a.project.name.localeCompare(b.project.name),
  )
}

/** What "Empty trash" deletes. */
export function trashTotals(groups: TrashGroup[]): { projects: number; sessions: number } {
  return {
    projects: groups.filter((g) => g.projectTrashed).length,
    sessions: groups.reduce((n, g) => n + g.purgeCount, 0),
  }
}

/** The stored sort choice, or the default (time deleted). */
export function parseTrashSort(value: string | null | undefined): TrashSort {
  return (TRASH_SORTS as readonly string[]).includes(value ?? '') ? (value as TrashSort) : 'deleted'
}
