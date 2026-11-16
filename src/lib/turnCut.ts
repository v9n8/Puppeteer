// The "Stopped mid-turn" mark (V3-29). The backend writes one `_puppeteer_turn_cut`
// record at the end of every turn that was cut short (src-tauri/src/runs/turn_cut.rs);
// this file reads it and words it. Kept free of Vue / DOM / `@/` imports so it can be
// unit-tested with `node --test`.

export const TURN_CUT_TYPE = '_puppeteer_turn_cut'

/** Why the turn was cut; '' when the log does not say (an older log). */
export type TurnCutReason = 'user' | 'quota' | 'agent' | 'app_closed' | ''

export interface TurnCut {
  reason: TurnCutReason
  /** ISO time the cut was recorded; '' when unknown. */
  at: string
}

type Translate = (key: string, params?: Record<string, unknown>) => string

const REASONS: readonly string[] = ['user', 'quota', 'agent', 'app_closed']

function validTime(v: unknown): string {
  return typeof v === 'string' && !Number.isNaN(new Date(v).getTime()) ? v : ''
}

/** One log record as a mark; an unknown reason still marks the cut, without a cause. */
export function parseTurnCut(e: Record<string, unknown>): TurnCut {
  const reason = typeof e.reason === 'string' && REASONS.includes(e.reason) ? (e.reason as TurnCutReason) : ''
  return { reason, at: validTime(e.at) }
}

/** The time of a mark: the clock when it is today, the date and clock otherwise. */
export function turnCutTime(at: string, now: Date): string {
  const d = new Date(at)
  if (d.toDateString() === now.toDateString()) {
    return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
  }
  return d.toLocaleString(undefined, { month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}

/** "Stopped mid-turn · you pressed Cancel at 10:52"; both drop out when the cause or time is unknown. */
export function turnCutLabel(cut: TurnCut, now: Date, t: Translate): string {
  const title = t('streamLog.turnCut.title')
  if (!cut.reason || !cut.at) return title
  return `${title} ${t(`streamLog.turnCut.${cut.reason}`, { time: turnCutTime(cut.at, now) })}`
}

/** Hover text: the full date and time; for `app_closed` it says the time is when the app noticed. */
export function turnCutTooltip(cut: TurnCut, t: Translate): string {
  if (!cut.at) return ''
  const full = new Date(cut.at).toLocaleString()
  return cut.reason === 'app_closed' ? t('streamLog.turnCut.appClosedHint', { time: full }) : full
}

interface EntryLike {
  kind: string
}

interface RunLike {
  status?: string | null
  end_reason?: string | null
  finished_at?: string | null
}

/**
 * A log written before the marks existed: its run was cancelled and its last turn
 * has no mark, so the run's own record stands in for it (the reason only `quota` /
 * `app_closed` are known; a Cancel left none). Earlier turns have no data, so they
 * get nothing.
 */
export function fallbackLastTurnCut(entries: readonly EntryLike[], run: RunLike | null | undefined): TurnCut | null {
  if (!run || run.status !== 'cancelled' || entries.length === 0) return null
  for (let i = entries.length - 1; i >= 0; i--) {
    const kind = entries[i].kind
    if (kind === 'turn_cut') return null
    if (kind === 'user') break
  }
  if (run.end_reason !== 'quota' && run.end_reason !== 'app_closed') return { reason: '', at: '' }
  return { reason: run.end_reason, at: validTime(run.finished_at) }
}
