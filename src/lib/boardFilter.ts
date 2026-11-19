// The board's column filter (UR-60): which columns the flag board shows and in what
// order. "No flag" first, then one group per source ("shared", then each Project's
// own flags), each group in the user's order. Kept per scope (All projects, or one
// Project id) in localStorage on this machine; it never changes the flag order in
// Settings. Pure: no Vue, Tauri or `@/` imports, so `node --test` can load it.
import type { SessionFlag } from './sessionFlags.ts'
import { boardColumns, NO_FLAG_COLUMN, type BoardColumn } from './flagBoard.ts'

export const BOARD_FILTER_KEY = 'puppeteer.boardFilter'
/** The group of the shared flags; every other group is a Project id. */
export const SHARED_GROUP = 'shared'
/** The scope of the All projects board; every other scope is a Project id. */
export const ALL_SCOPE = 'all'

export interface BoardFilter {
  /** Columns shown: `none` and flag ids. */
  on: string[]
  /** Group order: `shared` and Project ids. */
  groups: string[]
  /** Flag order inside each group. */
  flags: Record<string, string[]>
}

export type SavedFilters = Record<string, BoardFilter>

const groupOf = (flag: Pick<SessionFlag, 'project_id'>): string => flag.project_id ?? SHARED_GROUP

/**
 * The flags the filter lists, in Settings order: one Project lists the shared flags
 * it does not hide and its own; All projects lists every flag it was given.
 */
export function listedFlags(flags: readonly SessionFlag[], scope: string): SessionFlag[] {
  const mode = scope === ALL_SCOPE ? 'all' : 'project'
  return boardColumns(flags, mode)
    .map((c) => c.flag)
    .filter((f): f is SessionFlag => !!f && (mode === 'all' || f.project_id === null || f.project_id === scope))
}

/**
 * What a scope shows before the user picks anything: every shared flag on, "No
 * flag" and the own flags off. Groups: shared, then the Projects in `projectOrder`
 * (the Project list), then any other Project in the order its flags came.
 */
export function defaultFilter(flags: readonly SessionFlag[], scope: string, projectOrder: readonly string[] = []): BoardFilter {
  const listed = listedFlags(flags, scope)
  const present = [...new Set(listed.map(groupOf).filter((g) => g !== SHARED_GROUP))]
  const groups = [
    SHARED_GROUP,
    ...projectOrder.filter((p) => present.includes(p)),
    ...present.filter((p) => !projectOrder.includes(p)),
  ]
  const byGroup: Record<string, string[]> = {}
  for (const g of groups) byGroup[g] = listed.filter((f) => groupOf(f) === g).map((f) => f.id)
  return { on: listed.filter((f) => f.project_id === null).map((f) => f.id), groups, flags: byGroup }
}

/**
 * A saved filter brought up to date with the flags there are now: a flag or Project
 * gone is dropped; a new flag joins the end of its group (on when shared, off when a
 * Project's own); a new Project's group joins the end. Nothing saved: the default.
 */
export function reconcileFilter(
  saved: BoardFilter | null | undefined,
  flags: readonly SessionFlag[],
  scope: string,
  projectOrder: readonly string[] = [],
): BoardFilter {
  const base = defaultFilter(flags, scope, projectOrder)
  if (!saved) return base
  const known = new Set(Object.values(saved.flags).flat())
  const groups = saved.groups.filter((g, i) => base.groups.includes(g) && saved.groups.indexOf(g) === i)
  for (const g of base.groups) if (!groups.includes(g)) groups.push(g)
  const byGroup: Record<string, string[]> = {}
  for (const g of groups) {
    const kept = (saved.flags[g] ?? []).filter((id, i, all) => base.flags[g].includes(id) && all.indexOf(id) === i)
    byGroup[g] = [...kept, ...base.flags[g].filter((id) => !kept.includes(id))]
  }
  const listed = new Set(Object.values(byGroup).flat())
  const on = saved.on.filter((id, i) => (id === NO_FLAG_COLUMN || listed.has(id)) && saved.on.indexOf(id) === i)
  for (const id of base.on) if (!known.has(id) && !on.includes(id)) on.push(id)
  return { on, groups, flags: byGroup }
}

/** The columns the board shows, left to right: "No flag" when on, then each group's flags that are on. */
export function filterColumns(filter: BoardFilter, flags: readonly SessionFlag[], scope: string): BoardColumn[] {
  const all = boardColumns(flags, scope === ALL_SCOPE ? 'all' : 'project')
  const listed = new Set(listedFlags(flags, scope).map((f) => f.id))
  const byId = new Map(all.filter((c) => c.kind === 'none' || listed.has(c.id)).map((c) => [c.id, c]))
  const on = new Set(filter.on)
  const ids = [NO_FLAG_COLUMN, ...filter.groups.flatMap((g) => filter.flags[g] ?? [])]
  return ids.filter((id) => on.has(id)).map((id) => byId.get(id)).filter((c): c is BoardColumn => !!c)
}

/** The filter shows what the default would (the badge on the Filter button hides then). */
export function isDefaultFilter(
  filter: BoardFilter,
  flags: readonly SessionFlag[],
  scope: string,
  projectOrder: readonly string[] = [],
): boolean {
  const ids = (f: BoardFilter) => filterColumns(f, flags, scope).map((c) => c.id).join('\n')
  return ids(filter) === ids(defaultFilter(flags, scope, projectOrder))
}

/** Same ticks (in any order), same group order, same flag order in every group. */
export function sameFilter(a: BoardFilter, b: BoardFilter): boolean {
  const on = new Set(a.on)
  if (on.size !== new Set(b.on).size || b.on.some((id) => !on.has(id))) return false
  if (a.groups.join('\n') !== b.groups.join('\n')) return false
  return a.groups.every((g) => (a.flags[g] ?? []).join('\n') === (b.flags[g] ?? []).join('\n'))
}

/** The filter with one column ticked or unticked. */
export function toggleColumn(filter: BoardFilter, id: string): BoardFilter {
  const on = filter.on.includes(id) ? filter.on.filter((x) => x !== id) : [...filter.on, id]
  return { ...filter, on }
}

/** How many cards have no column on the board (the status line counts them). */
export function hiddenCount(cards: readonly { flag_id?: string | null }[], columns: readonly BoardColumn[]): number {
  const ids = new Set(columns.map((c) => c.id))
  return cards.filter((c) => !ids.has(c.flag_id || NO_FLAG_COLUMN)).length
}

const isStrings = (v: unknown): v is string[] => Array.isArray(v) && v.every((x) => typeof x === 'string')

function isFilter(v: unknown): v is BoardFilter {
  if (!v || typeof v !== 'object') return false
  const f = v as Record<string, unknown>
  if (!isStrings(f.on) || !isStrings(f.groups) || !f.flags || typeof f.flags !== 'object') return false
  return Object.values(f.flags as Record<string, unknown>).every(isStrings)
}

/** The saved filters by scope; a broken entry (or a broken whole) is left out, so its scope gets the default. */
export function parseFilter(raw: string | null): SavedFilters {
  if (!raw) return {}
  let data: unknown
  try {
    data = JSON.parse(raw)
  } catch {
    return {}
  }
  if (!data || typeof data !== 'object' || Array.isArray(data)) return {}
  const out: SavedFilters = {}
  for (const [scope, f] of Object.entries(data as Record<string, unknown>)) if (isFilter(f)) out[scope] = f
  return out
}

export function serializeFilter(saved: SavedFilters): string {
  return JSON.stringify(saved)
}
