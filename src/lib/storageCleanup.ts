// Storage cleanup (Stats › Storage, Settings › Storage): the shapes the backend
// returns (`storage_cleanup`) and the pure helpers the table, the preview dialog
// and the settings section share. Kept free of Vue and i18n so it can be tested
// with node --test.

/** One kind in a cleanup preview: what the next cleanup would take. */
export interface CleanupCategory {
  id: string
  items: number
  files: number
  bytes: number
  /** Old enough, but kept (worktrees with unfinished work, …). */
  kept_items: number
  kept_bytes: number
}

export interface LastCleanup {
  /** RFC 3339. */
  at: string
  trigger: 'auto' | 'manual'
  files: number
  bytes: number
  errors: number
}

export interface CleanupPreview {
  /** N as stored; 0 = auto-clean off. */
  days: number
  /** The age a cleanup started now uses (30 when N is 0). */
  effective_days: number
  busy: boolean
  categories: CleanupCategory[]
  last: LastCleanup | null
}

export interface CleanupOutcome {
  files: number
  bytes: number
  errors: number
  per_category: { id: string; items: number; files: number; bytes: number; errors: number; kept: number }[]
}

export interface Compacted {
  before_bytes: number
  after_bytes: number
}

/** The order the preview dialog lists the kinds in: biggest usual savings first. */
export const CLEANUP_ORDER = [
  'trash',
  'worktrees',
  'snapshots',
  'deleted_accounts',
  'sidecar_junk',
  'puppeteer_task_orphans',
  'pastes',
  'app_logs',
]

/** Kinds whose size is an estimate (a Trash run's transcript copies in other accounts are not counted). */
export const ESTIMATED = new Set(['trash'])

/** What the "Cleanable" cell of a Storage row shows; `null` for a row the cleanup never touches. */
export interface Cleanable {
  files: number
  bytes: number
  keptItems: number
  keptBytes: number
}

/** Each Storage row's share of the preview, by row id. */
export function mergeCleanable(
  rowIds: string[],
  preview: Pick<CleanupPreview, 'categories'> | null,
): Record<string, Cleanable | null> {
  const byId = new Map((preview?.categories ?? []).map((c) => [c.id, c]))
  const out: Record<string, Cleanable | null> = {}
  for (const id of rowIds) {
    const c = byId.get(id)
    out[id] = c ? { files: c.files, bytes: c.bytes, keptItems: c.kept_items, keptBytes: c.kept_bytes } : null
  }
  return out
}

/** The preview's kinds in [`CLEANUP_ORDER`], unknown ones last, with the sum. */
export function previewRows(categories: CleanupCategory[]): {
  rows: CleanupCategory[]
  files: number
  bytes: number
} {
  const rank = (id: string) => {
    const i = CLEANUP_ORDER.indexOf(id)
    return i < 0 ? CLEANUP_ORDER.length : i
  }
  const rows = [...categories].sort((a, b) => rank(a.id) - rank(b.id))
  return {
    rows,
    files: rows.reduce((n, c) => n + c.files, 0),
    bytes: rows.reduce((n, c) => n + c.bytes, 0),
  }
}

/** The last cleanup, ready to word; `null` when there is none or it cannot be read. */
export function formatLast(
  last: LastCleanup | null | undefined,
): { at: Date; trigger: 'auto' | 'manual'; files: number; bytes: number; errors: number } | null {
  if (!last) return null
  const at = new Date(last.at)
  if (Number.isNaN(at.getTime())) return null
  return { at, trigger: last.trigger === 'auto' ? 'auto' : 'manual', files: last.files, bytes: last.bytes, errors: last.errors }
}

/** A stored N as the backend reads it: 0 (off) or 7–3650, anything else → 30. */
export function effectiveDays(value: string | number | null | undefined): number {
  const n = typeof value === 'number' ? value : Number(String(value ?? '').trim() || NaN)
  if (!Number.isInteger(n)) return 30
  if (n === 0) return 0
  return n >= 7 && n <= 3650 ? n : 30
}

/** Whether `value` is one the backend keeps as typed. */
export function isValidDays(value: string | number | null | undefined): boolean {
  const text = String(value ?? '').trim()
  return /^\d+$/.test(text) && effectiveDays(text) === Number(text)
}
