// Quota warning tier (Settings › Sessions › Quota): which plan windows should
// raise a toast now. The backend reports the windows at or past
// `quota_warn_percent` (`quota_warn_windows` of a budget status); this decides
// which of them are new, so each window warns once per crossing.

/** One plan window at or past the warning tier, as the backend reports it. */
export interface QuotaWarnWindow {
  /** '5h' | 'week'. */
  period: string
  percent: number
  reset: string | null
}

/** One usage row (an engine, or one Claude account) and its warned windows. */
export interface QuotaWarnRow {
  /** Stable row id, e.g. `claude:<accountId>` or `codex`. */
  key: string
  /** `undefined` while the row has no status yet: its latch is left alone. */
  windows: QuotaWarnWindow[] | undefined
}

export interface QuotaWarnCrossing {
  key: string
  period: string
  percent: number
}

/**
 * The windows that crossed into the warning tier since the last call. `latch`
 * holds `<row key>|<period>` of the windows already warned and is updated in
 * place: a window leaves it once it is no longer reported (its % dropped below
 * the tier, the window reset, or the tier was switched off), so the next
 * crossing warns again. The reset time is not part of the identity because a
 * fresh snapshot may move it slightly within the same window.
 */
export function takeQuotaWarnCrossings(latch: Set<string>, rows: QuotaWarnRow[]): QuotaWarnCrossing[] {
  const crossings: QuotaWarnCrossing[] = []
  for (const row of rows) {
    if (!row.windows) continue
    const prefix = `${row.key}|`
    const current = new Set(row.windows.map((w) => prefix + w.period))
    for (const id of [...latch]) {
      if (id.startsWith(prefix) && !current.has(id)) latch.delete(id)
    }
    for (const w of row.windows) {
      const id = prefix + w.period
      if (latch.has(id)) continue
      latch.add(id)
      crossings.push({ key: row.key, period: w.period, percent: w.percent })
    }
  }
  return crossings
}
