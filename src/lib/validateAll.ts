// Pure helpers for Settings → Connections "Validate all": the summary toast and
// the sequential walk over the rows being shown.
import type { CheckResult } from '@/lib/connectionTypes'

export interface CheckSummary {
  ok: number
  needsLogin: number
  failed: number
}

/** Counts the results of one run. A `warn` still validated; a row that was
 *  skipped (no result) is not counted. */
export function summarizeChecks(results: readonly (CheckResult | undefined)[]): CheckSummary {
  const sum: CheckSummary = { ok: 0, needsLogin: 0, failed: 0 }
  for (const r of results) {
    if (!r) continue
    if (r.outcome === 'ok' || r.outcome === 'warn') sum.ok++
    else if (r.outcome === 'needsLogin') sum.needsLogin++
    else sum.failed++
  }
  return sum
}

/** The row to validate at `index`, or null when the walk is done. */
export function nextRow<T>(rows: readonly T[], index: number): T | null {
  return index >= 0 && index < rows.length ? rows[index] : null
}
