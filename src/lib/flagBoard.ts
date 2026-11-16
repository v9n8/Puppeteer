// The flag board (V3-22): which columns it has, which column a card sits in, who may
// drop where, the filter box and the keyboard (lift / move / drop). Pure: no Vue,
// Tauri or `@/` imports, so `node --test` can load it. The backend
// (commands/flag_board.rs) lists the sessions and flags; `set_run_flag` is the
// only write and enforces the same drop rule again.
import type { SessionFlag } from './sessionFlags.ts'

/** One project's board, or every project's. */
export type BoardMode = 'project' | 'all'

/** The id of the "No flag" column, and of the "Own flags" column of the All projects board. */
export const NO_FLAG_COLUMN = 'none'
export const OWN_FLAGS_COLUMN = 'own'

/** A shared flag one project hides. */
export interface HiddenMark {
  project_id: string
  flag_id: string
}

export interface BoardColumn {
  id: string
  kind: 'none' | 'flag' | 'own'
  /** The flag a `flag` column stands for. */
  flag: SessionFlag | null
  /** False: nothing can be dropped here (the All projects board's "Own flags" column). */
  droppable: boolean
}

/**
 * Columns left to right: "No flag"; the shared flags in Settings order, those the
 * project hides left out; then the project's own. The All projects board lists
 * every shared flag, then one "Own flags" column that takes no drop.
 */
export function boardColumns(flags: readonly SessionFlag[], mode: BoardMode): BoardColumn[] {
  const none: BoardColumn = { id: NO_FLAG_COLUMN, kind: 'none', flag: null, droppable: true }
  const flagColumn = (flag: SessionFlag): BoardColumn => ({ id: flag.id, kind: 'flag', flag, droppable: true })
  const shared = flags.filter((f) => f.project_id === null)
  if (mode === 'all') {
    return [none, ...shared.map(flagColumn), { id: OWN_FLAGS_COLUMN, kind: 'own', flag: null, droppable: false }]
  }
  const own = flags.filter((f) => f.project_id !== null)
  return [none, ...[...shared, ...own].filter((f) => !f.hidden).map(flagColumn)]
}

/**
 * The cards of every column, in the order given. A card whose flag has no column
 * (a flag its project hides since) sits in "No flag"; on the All projects board a
 * flag without a shared column is a project's own.
 */
export function groupCards<C extends { flag_id?: string | null }>(
  cards: readonly C[],
  columns: readonly BoardColumn[],
): Map<string, C[]> {
  const grouped = new Map<string, C[]>(columns.map((c) => [c.id, []]))
  const hasOwn = grouped.has(OWN_FLAGS_COLUMN)
  for (const card of cards) {
    const id = card.flag_id
    const target = id && grouped.has(id) ? id : id && hasOwn ? OWN_FLAGS_COLUMN : NO_FLAG_COLUMN
    grouped.get(target)?.push(card)
  }
  return grouped
}

export interface FilteredColumn<C> {
  cards: C[]
  matched: number
  total: number
}

/** The cards whose name contains `query` (any case); `matched` / `total` is the count in the column head. */
export function filterCards<C>(
  grouped: ReadonlyMap<string, C[]>,
  query: string,
  label: (card: C) => string,
): Map<string, FilteredColumn<C>> {
  const q = query.trim().toLowerCase()
  const out = new Map<string, FilteredColumn<C>>()
  for (const [id, cards] of grouped) {
    const shown = q ? cards.filter((c) => label(c).toLowerCase().includes(q)) : cards
    out.set(id, { cards: shown, matched: shown.length, total: cards.length })
  }
  return out
}

/**
 * Whether `card` may be dropped on `column`: never on the "Own flags" column; a
 * shared flag not when the card's project hides it; a project's own flag only for
 * that project's cards (the backend refuses the rest too).
 */
export function canDrop(
  card: { project_id: string },
  column: BoardColumn,
  hidden: readonly HiddenMark[],
): boolean {
  if (!column.droppable) return false
  const flag = column.flag
  if (!flag) return true
  if (flag.project_id !== null) return flag.project_id === card.project_id
  return !hidden.some((h) => h.project_id === card.project_id && h.flag_id === flag.id)
}

// ---- Keyboard ----------------------------------------------------------------------

/** What the board knows for the keyboard: its columns and the visible cards of each. */
export interface BoardLayout {
  columnIds: string[]
  cardIds: Record<string, string[]>
  /** Columns a lifted card can land on. */
  dropColumnIds: string[]
}

export interface BoardKeyState {
  /** The card with focus. */
  cursor: string | null
  /** The card Space lifted, with the column it would land on. */
  lifted: { runId: string; from: string; target: string } | null
}

export type BoardKeyEffect =
  | { kind: 'open'; runId: string }
  | { kind: 'drop'; runId: string; columnId: string }
  | { kind: 'cancel' }
  | { kind: 'escape' }
  | null

export interface BoardKeyResult {
  state: BoardKeyState
  effect: BoardKeyEffect
}

function columnOf(layout: BoardLayout, runId: string | null): string | null {
  if (!runId) return null
  return layout.columnIds.find((c) => layout.cardIds[c]?.includes(runId)) ?? null
}

/**
 * One key press. Up / Down move the focus along the column (Left / Right to the
 * next column with cards); Enter opens the card; Space lifts it, Left / Right then
 * pick the column (those that take a drop), Space drops, Esc puts it back. Esc with
 * nothing lifted is `escape` (the caller closes its menu). Any other key: unchanged.
 */
export function moveSelection(layout: BoardLayout, state: BoardKeyState, key: string): BoardKeyResult {
  const same: BoardKeyResult = { state, effect: null }
  const { lifted } = state

  if (key === 'Escape') {
    return lifted
      ? { state: { ...state, lifted: null }, effect: { kind: 'cancel' } }
      : { state, effect: { kind: 'escape' } }
  }

  if (lifted) {
    if (key === 'ArrowLeft' || key === 'ArrowRight') {
      const at = layout.dropColumnIds.indexOf(lifted.target)
      const next = layout.dropColumnIds[at + (key === 'ArrowRight' ? 1 : -1)]
      return next ? { state: { ...state, lifted: { ...lifted, target: next } }, effect: null } : same
    }
    if (key === ' ') {
      const next: BoardKeyState = { ...state, lifted: null }
      if (lifted.target === lifted.from) return { state: next, effect: { kind: 'cancel' } }
      return { state: next, effect: { kind: 'drop', runId: lifted.runId, columnId: lifted.target } }
    }
    return same
  }

  const column = columnOf(layout, state.cursor)
  if (key === 'Enter') return state.cursor ? { state, effect: { kind: 'open', runId: state.cursor } } : same
  if (key === ' ') {
    if (!state.cursor || !column) return same
    return { state: { ...state, lifted: { runId: state.cursor, from: column, target: column } }, effect: null }
  }
  if (!state.cursor || !column) return same
  const list = layout.cardIds[column] ?? []
  const at = list.indexOf(state.cursor)
  if (key === 'ArrowDown' || key === 'ArrowUp') {
    const next = list[at + (key === 'ArrowDown' ? 1 : -1)]
    return next ? { state: { ...state, cursor: next }, effect: null } : same
  }
  if (key === 'ArrowLeft' || key === 'ArrowRight') {
    const step = key === 'ArrowRight' ? 1 : -1
    for (let i = layout.columnIds.indexOf(column) + step; i >= 0 && i < layout.columnIds.length; i += step) {
      const cards = layout.cardIds[layout.columnIds[i]] ?? []
      if (cards.length) return { state: { ...state, cursor: cards[Math.min(at, cards.length - 1)] }, effect: null }
    }
  }
  return same
}

// ---- Card and text helpers ---------------------------------------------------------

export type CardStatus = 'running' | 'waiting' | 'done' | 'failed' | 'cancelled' | 'draft'

/** The dot of a card: waiting for an answer beats the stored status; a fresh session that never ran is a draft. */
export function cardStatus(run: { status: string; run_type: string }, waiting: boolean): CardStatus {
  if (waiting) return 'waiting'
  if (run.status === 'fetched' && run.run_type === 'session') return 'draft'
  if (run.status === 'running' || run.status === 'done' || run.status === 'failed' || run.status === 'cancelled') return run.status
  return 'draft'
}

/** The name on a card; the same text as the History row (`runLabel`). */
export function cardTitle(
  run: { title?: string | null; run_type: string; ref_number: number | null },
  t: (key: string) => string,
): string {
  if (run.title?.trim()) return run.title.trim()
  if (run.run_type === 'session') return t('run.labelSession')
  return `${run.run_type === 'analyze_issue' ? t('run.labelIssue') : t('run.labelPr')} #${run.ref_number}`
}

/**
 * What the board shows: the saved choice (a Project id, or 'all') when it is still
 * valid, else the Project of the tab in use, else the first Project; 'all' with none.
 * A Project deleted or in the Trash is no longer in `projectIds`.
 */
export function resolveBoardProject(saved: string | null, projectIds: readonly string[], tabProject: string | null): string {
  if (saved === 'all') return 'all'
  if (saved && projectIds.includes(saved)) return saved
  if (tabProject && projectIds.includes(tabProject)) return tabProject
  return projectIds[0] ?? 'all'
}

/** The flag a drop on this column sets: its own id, or null ("No flag"). */
export function dropFlagId(column: BoardColumn): string | null {
  return column.kind === 'flag' && column.flag ? column.flag.id : null
}
