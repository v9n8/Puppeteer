// Settings › Routing (QO-06): the routing report the backend builds from its
// recorded decisions (src-tauri/src/runs/routing/report.rs) and the cells the
// table shows. Kept free of Vue / DOM / `@/` imports so it can be unit-tested
// with `node --test`.

import { formatTokensShort } from './contextLimits.ts'

/** The switches of the section; unset = on, only `"false"` is off. */
export const ROUTING_SWITCHES = ['routing.policy_enabled', 'routing.escalation_enabled', 'routing.auto_update'] as const
export type RoutingSwitch = (typeof ROUTING_SWITCHES)[number]

/** The periods the report offers, in days. */
export const ROUTING_PERIODS = [7, 30] as const
export type RoutingPeriod = (typeof ROUTING_PERIODS)[number]

/** One line of the report: a task kind on one tier. */
export interface RoutingGroup {
  task_kind: string
  /** Model family (or id); empty = not known. */
  model: string
  /** Empty = not set. */
  effort: string
  runs: number
  /** Runs with a success or a failure: the base of the success rate. */
  counted: number
  successes: number
  avg_tokens: number | null
  retried: number
  by_agent: number
}

/** `Agent` calls of sessions that had a policy, and how many named a model, an effort, both. */
export interface RoutingAdoption {
  calls: number
  with_model: number
  with_effort: number
  with_both: number
}

export interface RoutingReport {
  days: number
  groups: RoutingGroup[]
  adoption: RoutingAdoption
}

/** Whether a switch's stored value is on. */
export function switchOn(value: string | undefined): boolean {
  return (value ?? '').trim() !== 'false'
}

/** `"95%"`, rounded; `"—"` when there is nothing to divide by. */
export function percent(part: number, whole: number): string {
  if (!whole || whole <= 0) return '—'
  return `${Math.round((100 * part) / whole)}%`
}

/** The cells of one table line, in the column order of the section. */
export interface RoutingReportCells {
  kind: string
  model: string
  effort: string
  runs: string
  success: string
  avgTokens: string
  retried: string
  byAgent: string
}

/**
 * One report line as the table shows it. The success rate is over the runs that
 * count (a stopped run or a worker that named no check does not); the retry and
 * the agent's-choice rates are over every run.
 */
export function routingReportCells(g: RoutingGroup): RoutingReportCells {
  return {
    kind: g.task_kind,
    model: g.model || '—',
    effort: g.effort || '—',
    runs: String(g.runs),
    success: percent(g.successes, g.counted),
    avgTokens: g.avg_tokens == null ? '—' : formatTokensShort(g.avg_tokens),
    retried: percent(g.retried, g.runs),
    byAgent: percent(g.by_agent, g.runs),
  }
}

/** The three QO-NFR13 rates: calls with a model, with an effort, with both. */
export function adoptionRates(a: RoutingAdoption): { model: string; effort: string; both: string } {
  return {
    model: percent(a.with_model, a.calls),
    effort: percent(a.with_effort, a.calls),
    both: percent(a.with_both, a.calls),
  }
}

// ---- Policy versions (QO-T17; src-tauri/src/runs/routing/update.rs) ----------

export interface RoutingTier {
  model: string
  effort: string
}

/** The counted results a move rests on. */
export interface RoutingBasis {
  samples: number
  successes: number
  failures: number
}

/** One task kind's move; `basis` is null for a version made by going back. */
export interface RoutingChange {
  kind: string
  from: RoutingTier | null
  to: RoutingTier | null
  basis: RoutingBasis | null
}

export interface RoutingPolicyVersion {
  version: number
  created_at: string
  /** `default` | `auto` | `revert`. */
  source: string
  changes: RoutingChange[]
  /** The full policy text new sessions of this version read. */
  text: string
}

/** Changes a recompute found while automatic updates were off. */
export interface RoutingProposal {
  based_on: number
  created_at: string
  changes: RoutingChange[]
}

export interface RoutingPolicies {
  /** Newest first: the first is the current one. */
  versions: RoutingPolicyVersion[]
  proposal: RoutingProposal | null
}

/** An automatic version the user has not been told about (`take_routing_notice`). */
export interface RoutingNotice {
  version: number
  changes: RoutingChange[]
}

/** What "Update now" did. */
export type RoutingRecomputed =
  | { result: 'skipped' | 'unchanged' }
  | { result: 'created'; version: number; changes: RoutingChange[] }
  | { result: 'proposed'; changes: RoutingChange[] }

/** Emitted by the backend when a recompute made a new version. */
export const ROUTING_UPDATED_EVENT = 'routing:policy_updated'

/** `sonnet/low`; a kind with no tier (`other`) reads `—`. */
export function tierText(tier: RoutingTier | null): string {
  return tier ? `${tier.model}/${tier.effort}` : '—'
}

/** `test: sonnet/medium → sonnet/low`. */
export function changeText(c: RoutingChange): string {
  return `${c.kind}: ${tierText(c.from)} → ${tierText(c.to)}`
}

/** The `{summary}` of the update toast: every move, `; ` between. */
export function noticeSummary(changes: RoutingChange[]): string {
  return changes.map(changeText).join('; ')
}

/** i18n key of a version's source; null for one this build does not know (shown as stored). */
export function sourceLabelKey(source: string): string | null {
  return source === 'default' || source === 'auto' || source === 'revert' ? `routing.versions.sources.${source}` : null
}

/**
 * Whether a notice is one to show: the backend hands each out once, and this
 * also keeps the window from showing a version again when the start-up read
 * and the event race.
 */
export function isNewNotice(notice: RoutingNotice | null | undefined, lastShown: number): notice is RoutingNotice {
  return !!notice && Number.isFinite(notice.version) && notice.version > lastShown
}
