// Search across sessions (SessionSearchView): the backend contract of
// `search_sessions` (src-tauri/src/commands/session_search.rs), grouping its hits
// per run, and the route query that opens a run at a hit (read back by RunView).
// Kept free of Vue / Tauri / `@/` imports so it can be unit-tested with `node --test`.
import { findMatches } from './pageFind.ts'

/** Prop gating the index, `search_sessions` and the Search screen. */
export const SESSION_SEARCH_FEATURE = 'sessions.cross_session_search'

/** `{ query, projectId? }` → {@link SessionSearch}. Read-only. */
export const CMD_SEARCH_SESSIONS = 'search_sessions'

/** One matching log record, as `search_sessions` returns it. */
export interface SessionSearchHit {
  run_id: string
  project_id: string
  project_name: string
  title: string | null
  engine: string
  run_type: string
  /** The run's History sort key: last worked on, else created. */
  activity_at: string
  /** Byte offset the record starts at — a `get_run_log_forward` cursor. */
  offset: number
  snippet: string
}

export interface SessionSearch {
  hits: SessionSearchHit[]
  /** More records matched than were returned. */
  truncated: boolean
}

/** The hits of one run. */
export interface SessionHitGroup {
  runId: string
  projectId: string
  projectName: string
  title: string | null
  engine: string
  runType: string
  activityAt: string
  hits: { offset: number; snippet: string }[]
}

/**
 * Hits grouped per run. Runs keep the order they first appear in (the backend
 * sorts most recently active first); each run's hits keep theirs (log order).
 */
export function groupSessionHits(hits: readonly SessionSearchHit[]): SessionHitGroup[] {
  const groups = new Map<string, SessionHitGroup>()
  for (const h of hits) {
    let g = groups.get(h.run_id)
    if (!g) {
      g = {
        runId: h.run_id,
        projectId: h.project_id,
        projectName: h.project_name,
        title: h.title,
        engine: h.engine,
        runType: h.run_type,
        activityAt: h.activity_at,
        hits: [],
      }
      groups.set(h.run_id, g)
    }
    g.hits.push({ offset: h.offset, snippet: h.snippet })
  }
  return [...groups.values()]
}

/** `snippet` cut into plain and matching parts, to highlight `query` (case-insensitive, like the search). */
export function snippetParts(snippet: string, query: string): { text: string; match: boolean }[] {
  const parts: { text: string; match: boolean }[] = []
  let at = 0
  for (const m of findMatches([snippet], query)) {
    if (m.start > at) parts.push({ text: snippet.slice(at, m.start), match: false })
    parts.push({ text: snippet.slice(m.start, m.end), match: true })
    at = m.end
  }
  if (at < snippet.length) parts.push({ text: snippet.slice(at), match: false })
  return parts
}

/** Where a hit opens its run: the record (`offset`) and the text to highlight (`find`). */
export interface RouteHit {
  offset: number
  find: string
}

/** Query of a run route (`project-run-detail`) that opens the run at a hit. */
export function hitRouteQuery(hit: RouteHit): Record<string, string> {
  return { offset: String(hit.offset), find: hit.find }
}

/** The hit a run route asks to open at ({@link hitRouteQuery}), or `null`. */
export function hitFromRouteQuery(query: Record<string, unknown>): RouteHit | null {
  const { offset, find } = query
  if (typeof offset !== 'string' || typeof find !== 'string' || !/^\d+$/.test(offset)) return null
  const n = Number(offset)
  return Number.isSafeInteger(n) ? { offset: n, find } : null
}
