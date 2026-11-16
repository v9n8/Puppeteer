// "Now: Work 62% · Personal 41%" under the block thresholds of Settings › Limits
// (V3-60): each account's current figure for the 5h or the weekly window, from
// the same plan snapshots the usage rings read (lib/usageRing).
import { usageSummary, type UsageWindow } from './usageRing.ts'

/** A snapshot older than this is shown as "(old)": the rings refresh on startup and on each run. */
export const QUOTA_NOW_STALE_MS = 60 * 60_000

export interface QuotaNowRow {
  label: string
  windows: UsageWindow[]
  /** `captured_at` of the snapshot; null when unknown. */
  capturedAt: string | null
}

export interface QuotaNowEntry {
  label: string
  percent: number
  /** The % may be from an old window or an old capture. */
  stale: boolean
  capturedAt: string | null
}

/**
 * The accounts that have a figure for `period`, in row order. For the weekly
 * period the most used weekly window counts, like the composer's account menu.
 * An account without that window is left out, so no data gives an empty list.
 */
export function quotaNowLine(
  rows: QuotaNowRow[],
  period: '5h' | 'week',
  now: number,
  staleAfterMs: number = QUOTA_NOW_STALE_MS,
): QuotaNowEntry[] {
  const out: QuotaNowEntry[] = []
  for (const row of rows) {
    const w = usageSummary(row.windows).find((x) => x.period === period)
    if (!w) continue
    const captured = row.capturedAt ? Date.parse(row.capturedAt) : NaN
    const old = !Number.isFinite(captured) || now - captured > staleAfterMs
    out.push({ label: row.label, percent: w.percent, stale: w.rolledOver || old, capturedAt: row.capturedAt })
  }
  return out
}
