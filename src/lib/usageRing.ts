// Usage rings on the session status bar (UX-23): one donut per account with
// plan usage (each Claude account, the shared Codex login and each Codex
// account). These helpers turn a stored plan
// snapshot (`get_plan_usage` / `get_plan_usage_codex`, the `/usage` and
// `/status` data) into the windows a ring and its card show, and decide their
// colour from the quota settings.

export type PlanWindowKey = 'five_hour' | 'seven_day' | 'seven_day_opus' | 'seven_day_sonnet'

/** Display order of the card rows. Codex only ever fills the first two. */
export const PLAN_WINDOW_KEYS: PlanWindowKey[] = ['five_hour', 'seven_day', 'seven_day_opus', 'seven_day_sonnet']

export interface PlanWindowSnapshot {
  utilization: number | null
  resets_at: string | null
  /** Live severity from the newest rate_limit_event: allowed | warning | blocked. */
  status?: string | null
}

/** The stored snapshot (`stats::PlanUsage`). */
export interface PlanUsageSnapshot {
  captured_at?: string | null
  rate_limits_available: boolean
  windows: Partial<Record<PlanWindowKey, PlanWindowSnapshot | null>>
}

export interface UsageWindow {
  key: PlanWindowKey
  /** The block threshold that applies: `budget_5h_percent` or `budget_week_percent`. */
  period: '5h' | 'week'
  /** Rounded % used; 0 once the window rolled over. */
  percent: number
  reset: string | null
  /** The reset time passed after the capture, so the stored % is from the old window. */
  rolledOver: boolean
  blocked: boolean
}

/**
 * The windows that have data, in display order. Mirrors the backend's
 * `plan_window_by_key`: no windows when plan limits don't apply (API key), a
 * window without a % is skipped, and a window whose reset already passed counts
 * as 0%.
 */
export function usageWindows(snapshot: PlanUsageSnapshot | null | undefined, now: number): UsageWindow[] {
  if (!snapshot?.rate_limits_available) return []
  const out: UsageWindow[] = []
  for (const key of PLAN_WINDOW_KEYS) {
    const w = snapshot.windows?.[key]
    if (!w) continue
    const resetMs = w.resets_at ? Date.parse(w.resets_at) : NaN
    const rolledOver = Number.isFinite(resetMs) && now > resetMs
    if (w.utilization == null && !rolledOver) continue
    out.push({
      key,
      period: key === 'five_hour' ? '5h' : 'week',
      percent: rolledOver ? 0 : Math.round(w.utilization ?? 0),
      reset: w.resets_at,
      rolledOver,
      blocked: !rolledOver && w.status === 'blocked',
    })
  }
  return out
}

/** The window the ring draws: the 5h window, else the most used one. */
export function ringWindow(windows: UsageWindow[]): UsageWindow | null {
  const fiveHour = windows.find((w) => w.key === 'five_hour')
  if (fiveHour) return fiveHour
  return windows.reduce<UsageWindow | null>((best, w) => (best && best.percent >= w.percent ? best : w), null)
}

/**
 * The figures an account row of the composer's account menu shows (UX-47): the
 * 5h window, then the most used weekly one (any weekly window over its block
 * threshold stops new turns). Codex may only have the weekly one.
 */
export function usageSummary(windows: UsageWindow[]): UsageWindow[] {
  const fiveHour = windows.find((w) => w.period === '5h')
  const week = windows
    .filter((w) => w.period === 'week')
    .reduce<UsageWindow | null>((best, w) => (best && best.percent >= w.percent ? best : w), null)
  return [fiveHour, week].filter((w): w is UsageWindow => !!w)
}

/** Amber starts here when the quota warning tier (`quota_warn_percent`) is off. */
export const DEFAULT_WARN_PERCENT = 80

export interface UsageThresholds {
  /** `quota_warn_percent`; 0 = off. */
  warnPercent: number
  /** `budget_5h_percent`; 0 = off. */
  block5hPercent: number
  /** `budget_week_percent`; 0 = off. */
  blockWeekPercent: number
}

/** A % setting as the backend reads it: 0 (off) when empty or not a number, else 0–100. */
export function parsePercentSetting(value: string | null | undefined): number {
  const text = String(value ?? '').trim()
  // Integers only, like Rust's `parse::<i64>()`: "85.5" or "85%" is off.
  if (!/^[+-]?\d+$/.test(text)) return 0
  return Math.min(100, Math.max(0, Number(text)))
}

export type UsageSeverity = 'ok' | 'warn' | 'over'

/**
 * Colour of one window. Red when the plan blocks it, it is at 100%, or it
 * reached its block threshold (the same check `enforce_budget` makes). Amber
 * from the quota warning tier, or from 80% while that tier is off.
 */
export function windowSeverity(w: UsageWindow, th: UsageThresholds): UsageSeverity {
  if (w.rolledOver) return 'ok'
  const block = w.period === '5h' ? th.block5hPercent : th.blockWeekPercent
  if (w.blocked || w.percent >= 100 || (block > 0 && w.percent >= block)) return 'over'
  const warn = th.warnPercent > 0 ? th.warnPercent : DEFAULT_WARN_PERCENT
  return w.percent >= warn ? 'warn' : 'ok'
}

const RANK: Record<UsageSeverity, number> = { ok: 0, warn: 1, over: 2 }

/**
 * Colour of a whole ring: its worst window. The ring draws the 5h %, but a
 * weekly window over its block threshold also stops new sessions, so it must
 * still turn the ring red.
 */
export function ringSeverity(windows: UsageWindow[], th: UsageThresholds): UsageSeverity {
  let worst: UsageSeverity = 'ok'
  for (const w of windows) {
    const s = windowSeverity(w, th)
    if (RANK[s] > RANK[worst]) worst = s
  }
  return worst
}

export type ResetCountdown =
  | { due: true }
  | { due: false; days: number; hours: number; minutes: number }

/**
 * Time left until `reset`, rounded up to the minute; `null` when the reset time
 * is unknown. Past days only hours are kept, so the text stays short ("3d 4h").
 */
export function resetCountdown(reset: string | null | undefined, now: number): ResetCountdown | null {
  if (!reset) return null
  const at = Date.parse(reset)
  if (!Number.isFinite(at)) return null
  const ms = at - now
  if (ms <= 0) return { due: true }
  const totalMinutes = Math.max(1, Math.ceil(ms / 60_000))
  const days = Math.floor(totalMinutes / 1440)
  const hours = Math.floor((totalMinutes % 1440) / 60)
  return { due: false, days, hours, minutes: days > 0 ? 0 : totalMinutes % 60 }
}

/**
 * The stored-snapshot read behind a usage row key (local DB, no engine call):
 * `claude` is the default Claude account (or the global `~/.claude`),
 * `codex` the shared `~/.codex` login, `<engine>:<accountId>` that account.
 */
export function planUsageRead(key: string): { cmd: string; args?: { accountId: string } } {
  const sep = key.indexOf(':')
  const engine = sep < 0 ? key : key.slice(0, sep)
  const cmd = engine === 'codex' ? 'get_plan_usage_codex' : 'get_plan_usage'
  return sep < 0 ? { cmd } : { cmd, args: { accountId: key.slice(sep + 1) } }
}

/** One ring on the status bar: a Claude account (or the single global Claude row) or a Codex login. */
export interface UsageRingAccount {
  /** Stable row id, e.g. `claude:<accountId>`, `claude`, `codex` or `codex:<accountId>` (same as the quota toast rows). */
  key: string
  label: string
  /** Engine name for the card header ("Claude" / "Codex"). */
  engine: string
  windows: UsageWindow[]
  capturedAt: string | null
  refreshing: boolean
  /** Last failed live probe, shown in the card. */
  refreshError: string | null
}

/**
 * The rings the status bar shows: every set-up account, with numbers or not
 * (an empty ring offers its refresh), so no account goes missing; the global
 * Claude row and the shared Codex login only once they have numbers.
 */
export function visibleUsageRings<R extends Pick<UsageRingAccount, 'key' | 'windows'>>(rows: readonly R[]): R[] {
  return rows.filter((r) => r.windows.length > 0 || r.key.includes(':'))
}
