// When a session is blocked or stopped by its quota (UX-48): the other accounts
// of the same engine to offer ("Switch to X and continue"), read from the stored
// plan snapshots the usage rings show.
import { usageWindows, windowSeverity, type PlanUsageSnapshot, type UsageThresholds } from './usageRing.ts'

export interface QuotaSwitchAccount {
  /** What the run's account is set to: an account id, or '' for the shared Codex login. */
  value: string
  label: string
  /** Usage row key: `claude:<id>`, `codex` or `codex:<id>`. */
  usageKey: string
}

/**
 * The accounts worth switching to, best first. An account is offered only
 * while every window is under its block threshold (not red on the usage
 * rings), ordered by its most used window, lowest first. An account with no
 * plan figures yet (never ran, or an API key) is not blocked by the guardrail
 * either, so it is offered after those. The run's own account (`currentKey`)
 * never is.
 */
export function quotaSwitchTargets(
  accounts: QuotaSwitchAccount[],
  currentKey: string,
  snapshots: Record<string, PlanUsageSnapshot | null | undefined>,
  thresholds: UsageThresholds,
  now: number,
): QuotaSwitchAccount[] {
  const ranked: { account: QuotaSwitchAccount; used: number }[] = []
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
