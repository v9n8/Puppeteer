// Pure helpers of the session info bar under the composer (V3-16): the counts
// and tooltip parts of the Connections and Links buttons, and the context level.

export type ConnectionKind = 'git' | 'servers' | 'aws' | 'azure'
export type LinkKind = 'slack' | 'todos' | 'notes'

export interface InfoPart<K extends string> {
  kind: K
  count: number
}

export interface InfoSummary<K extends string> {
  total: number
  /** Non-empty groups, in display order: the tooltip's "2 Git accounts, 1 server". */
  parts: InfoPart<K>[]
}

function summarize<K extends string>(order: readonly K[], counts: Record<K, number>): InfoSummary<K> {
  const parts = order
    .map((kind) => ({ kind, count: Math.max(0, counts[kind] || 0) }))
    .filter((p) => p.count > 0)
  return { total: parts.reduce((sum, p) => sum + p.count, 0), parts }
}

const CONNECTION_ORDER = ['git', 'servers', 'aws', 'azure'] as const
const LINK_ORDER = ['slack', 'todos', 'notes'] as const

/** Count behind the Connections button; `total` 0 means the button is not drawn. */
export function connectionSummary(counts: Record<ConnectionKind, number>): InfoSummary<ConnectionKind> {
  return summarize(CONNECTION_ORDER, counts)
}

/** Count behind the Links button. A group whose feature prop is off is neither listed nor counted. */
export function linkSummary(
  counts: Record<LinkKind, number>,
  enabled: Record<LinkKind, boolean>,
): InfoSummary<LinkKind> {
  return summarize(LINK_ORDER, {
    slack: enabled.slack ? counts.slack : 0,
    todos: enabled.todos ? counts.todos : 0,
    notes: enabled.notes ? counts.notes : 0,
  })
}

export type ContextLevel = 'ok' | 'warn' | 'over'

/** `over` once the window is full (100%), `warn` from the warning percent (default 80). */
export function contextLevel(pct: number, warnAt = 80): ContextLevel {
  if (pct >= 100) return 'over'
  return pct >= warnAt ? 'warn' : 'ok'
}
