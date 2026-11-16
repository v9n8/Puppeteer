// When a session is blocked or stopped by its quota (V3-36): the notice names the
// window that is over and the other account still usable, read from the stored
// plan snapshots the usage rings show. The app only tells; the user switches.
import { usageWindows, windowSeverity, type PlanUsageSnapshot, type UsageThresholds, type UsageWindow } from './usageRing.ts'

export interface QuotaAccount {
  /** What the run's account is set to: an account id, or '' for the shared Codex login. */
  value: string
  label: string
  /** Usage row key: `claude:<id>`, `codex` or `codex:<id>`. */
  usageKey: string
}

/**
 * The accounts still usable instead, best first. An account is offered only
 * while every window is under its block threshold (not red on the usage
 * rings), ordered by its most used window, lowest first. An account with no
 * plan figures yet (never ran, or an API key) is not blocked by the guardrail
 * either, so it is offered after those. The run's own account (`currentKey`)
 * never is.
 */
export function quotaAlternatives(
  accounts: QuotaAccount[],
  currentKey: string,
  snapshots: Record<string, PlanUsageSnapshot | null | undefined>,
  thresholds: UsageThresholds,
  now: number,
): QuotaAccount[] {
  const ranked: { account: QuotaAccount; used: number }[] = []
  for (const account of accounts) {
    if (account.usageKey === currentKey) continue
    const windows = usageWindows(snapshots[account.usageKey], now)
    if (windows.some((w) => windowSeverity(w, thresholds) === 'over')) continue
    const used = windows.length ? Math.max(...windows.map((w) => w.percent)) : Infinity
    ranked.push({ account, used })
  }
  // Array sort is stable: equal figures (two without data too) keep the
  // accounts' own order.
  return ranked.sort((a, b) => (a.used === b.used ? 0 : a.used - b.used)).map((r) => r.account)
}

export interface QuotaNoticeParts {
  /** Window of the run's account that is over; `period` picks its name. */
  window: UsageWindow['period']
  percent: number
  /** Block threshold of that window (0 = blocked by the plan or at 100%). */
  limit: number
  /** Best other account; `percent` null when it has no plan figures yet. */
  other: { label: string; percent: number | null } | null
}

/**
 * The facts of a quota notice: the most used window of the run's account that
 * is over, and the best alternative. Null when no window is over (the guardrail
 * tripped on something else), so the caller falls back to the plain message.
 */
export function quotaNoticeText(
  currentWindows: UsageWindow[],
  alternative: { account: QuotaAccount; windows: UsageWindow[] } | null,
  thresholds: UsageThresholds,
): QuotaNoticeParts | null {
  const over = currentWindows.filter((w) => windowSeverity(w, thresholds) === 'over')
  if (!over.length) return null
  const worst = over.reduce((a, b) => (b.percent > a.percent ? b : a))
  const other = alternative
    ? {
        label: alternative.account.label,
        percent: alternative.windows.length ? Math.max(...alternative.windows.map((w) => w.percent)) : null,
      }
    : null
  return {
    window: worst.period,
    percent: worst.percent,
    limit: worst.period === '5h' ? thresholds.block5hPercent : thresholds.blockWeekPercent,
    other,
  }
}
