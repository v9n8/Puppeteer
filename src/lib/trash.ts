// Trash screen (TrashView): the shapes of `list_trash` (src-tauri/src/commands/trash.rs)
// and the grouping / sorting of its rows, as pure functions (tested in
// __tests__/trash.test.mjs).
//
// A run is in the Trash when it or its project is trashed. Trashing a project
// does not stamp its runs, so such a run has `deleted_at: null` and takes the
// project's time.

import { foldText } from './commandPalette.ts'

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

export type TrashSort = 'deleted-desc' | 'deleted-asc' | 'created-desc' | 'created-asc'
export const TRASH_SORTS: readonly TrashSort[] = ['deleted-desc', 'deleted-asc', 'created-desc', 'created-asc']

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
  return time(sort.startsWith('deleted') ? s.deletedAt : s.run.created_at)
}

/** `1` for newest first, `-1` for oldest first. */
function direction(sort: TrashSort): 1 | -1 {
  return sort.endsWith('-asc') ? -1 : 1
}

/**
 * One group per project, ordered by `sort`; inside a group the sessions
 * are sorted the same way. A worker whose conductor is in the Trash too is
 * listed under it, since restoring or purging the conductor takes it along.
 */
export function groupTrash(listing: TrashListing, sort: TrashSort): TrashGroup[] {
  const dir = direction(sort)
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
      .sort((a, b) => dir * (sessionKey(b, sort) - sessionKey(a, sort)) || a.run.id.localeCompare(b.run.id))
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
      : time(sort.startsWith('deleted') ? g.project.deleted_at : g.project.created_at)
  return groups.sort(
    (a, b) => dir * (groupKey(b) - groupKey(a)) || a.project.name.localeCompare(b.project.name),
  )
}

/** What "Empty trash" deletes. */
export function trashTotals(groups: TrashGroup[]): { projects: number; sessions: number } {
  return {
    projects: groups.filter((g) => g.projectTrashed).length,
    sessions: groups.reduce((n, g) => n + g.purgeCount, 0),
  }
}

/** The stored sort choice, or the default (newest deleted first). The old values `deleted` / `created` mean newest first. */
export function parseTrashSort(value: string | null | undefined): TrashSort {
  if (value === 'deleted' || value === 'created') return `${value}-desc`
  return (TRASH_SORTS as readonly string[]).includes(value ?? '') ? (value as TrashSort) : 'deleted-desc'
}

// ── Search ──────────────────────────────────────────────────────────────────

/** One project block as shown: `group` is what is listed, `full` the whole block. */
export interface TrashEntry {
  group: TrashGroup
  full: TrashGroup
  /** Session rows the search matched by their own name; `null`: the block is not narrowed (no search, or its project name matched). */
  matched: ReadonlySet<string> | null
}

/**
 * Narrows the blocks to `query` (accents and case ignored) in project names and
 * session names. A project-name match keeps the block whole; a session-name
 * match keeps only that row, and a worker's name matches its conductor's row.
 * `labelOf` is the name shown for a run. An empty query keeps everything.
 */
export function filterTrash(
  groups: TrashGroup[],
  query: string,
  labelOf: (run: TrashRun) => string,
): TrashEntry[] {
  const needle = foldText(query.trim())
  if (!needle) return groups.map((g) => ({ group: g, full: g, matched: null }))
  const entries: TrashEntry[] = []
  for (const full of groups) {
    if (foldText(full.project.name).includes(needle)) {
      entries.push({ group: full, full, matched: null })
      continue
    }
    const hit = (r: TrashRun) => foldText(labelOf(r)).includes(needle)
    const sessions = full.sessions.filter((s) => hit(s.run) || s.workers.some((w) => hit(w)))
    if (sessions.length === 0) continue
    entries.push({
      group: { ...full, sessions },
      full,
      matched: new Set(sessions.map((s) => s.run.id)),
    })
  }
  return entries
}

export interface TrashActionTargets {
  /** The block's buttons act on a part of it: label them "n Session khớp". */
  partial: boolean
  /** Session rows the buttons act on. */
  sessions: TrashSession[]
  /** The block is handled as a project (`restore_project` / `purge_project`). */
  wholeProject: boolean
  /** Sessions a purge of the targets deletes (rows with their workers). */
  purgeCount: number
  /** Session rows of the block the buttons leave in the Trash. */
  remaining: number
  /** Restoring a part of a trashed project also restores it and these other rows that are in the Trash only through it. */
  restoredWithProject: number
}

/** What the whole-block buttons act on, given the rows a search matched (`null`: all). */
export function groupActionTargets(full: TrashGroup, matched: ReadonlySet<string> | null): TrashActionTargets {
  if (matched === null) {
    return {
      partial: false,
      sessions: full.sessions,
      wholeProject: full.projectTrashed,
      purgeCount: full.purgeCount,
      remaining: 0,
      restoredWithProject: 0,
    }
  }
  const sessions = full.sessions.filter((s) => matched.has(s.run.id))
  return {
    partial: true,
    sessions,
    wholeProject: false,
    purgeCount: sessions.reduce((n, s) => n + sessionsLost(s.run), 0),
    remaining: full.sessions.length - sessions.length,
    restoredWithProject: full.projectTrashed
      ? full.sessions.filter((s) => !matched.has(s.run.id) && s.run.deleted_at === null).length
      : 0,
  }
}

/** Blocks shown open: the stored choice per project id, else only the first block. */
export function initialOpenGroups(groups: TrashGroup[], stored: Readonly<Record<string, boolean>> | null): Set<string> {
  const open = new Set<string>()
  groups.forEach((g, i) => {
    if (stored?.[g.project.id] ?? i === 0) open.add(g.project.id)
  })
  return open
}

/** `text` cut into pieces, the ones matching `query` flagged (accents and case ignored). One unflagged piece when nothing matches. */
export function highlightParts(text: string, query: string): { text: string; hit: boolean }[] {
  const needle = foldText(query.trim())
  const folded = foldText(text)
  // Folding must keep the positions; otherwise show the text plain.
  const at = needle && folded.length === text.length ? folded.indexOf(needle) : -1
  if (at === -1) return [{ text, hit: false }]
  return [
    { text: text.slice(0, at), hit: false },
    { text: text.slice(at, at + needle.length), hit: true },
    { text: text.slice(at + needle.length), hit: false },
  ].filter((p) => p.text)
}

// ── Content search ──────────────────────────────────────────────────────────

/** Snippets shown per session row; `count` is every matching message. */
export const CONTENT_SNIPPETS_SHOWN = 2

export interface TrashContentHit {
  /** The first snippets of the row (at most {@link CONTENT_SNIPPETS_SHOWN}). */
  snippets: string[]
  /** Messages that matched, the row's workers' included. */
  count: number
}

/** Where the search box runs: by name (as you type) or in the log text (on Enter). */
export type TrashSearchMode = 'name' | 'content'

/** The stored search mode, or the default (name). */
export function parseTrashSearchMode(value: string | null | undefined): TrashSearchMode {
  return value === 'content' ? 'content' : 'name'
}

/**
 * Narrows the blocks to the sessions `hitGroups` (the Trash-scope hits of
 * `search_sessions`, grouped per run) matched. A worker's hits count for its
 * conductor's row when that is listed with it; a run that is not in the list
 * is ignored. Every block keeps `matched` set, so its buttons act on the
 * matching rows only (`groupActionTargets`).
 */
export function contentFilter(
  groups: TrashGroup[],
  hitGroups: readonly { runId: string; hits: readonly { snippet: string }[] }[],
): { entries: TrashEntry[]; hits: Map<string, TrashContentHit> } {
  const byRun = new Map(hitGroups.map((g) => [g.runId, g.hits]))
  const hits = new Map<string, TrashContentHit>()
  const entries: TrashEntry[] = []
  for (const full of groups) {
    const sessions: TrashSession[] = []
    for (const s of full.sessions) {
      const snippets = [s.run, ...s.workers].flatMap((r) => (byRun.get(r.id) ?? []).map((h) => h.snippet))
      if (snippets.length === 0) continue
      sessions.push(s)
      hits.set(s.run.id, { snippets: snippets.slice(0, CONTENT_SNIPPETS_SHOWN), count: snippets.length })
    }
    if (sessions.length === 0) continue
    entries.push({ group: { ...full, sessions }, full, matched: new Set(sessions.map((s) => s.run.id)) })
  }
  return { entries, hits }
}
