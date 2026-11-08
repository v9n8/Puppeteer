// Quote → composer (the shared selection toolbar's Quote button, views/RunView.vue,
// and from a file window through lib/quoteAppend.ts):
// a quote lands in the session's draft as a markdown blockquote, so the next turn
// sends it with whatever the user types around it. Kept free of Vue / Tauri / `@/`
// imports so it can be unit-tested with `node --test`.

/** Where a quote came from, shown as its first quoted line. */
export interface QuoteSource {
  path: string
  lines?: { start: number; end: number }
}

function sourceLabel(source: QuoteSource): string {
  const l = source.lines
  if (!l) return source.path
  return l.end > l.start ? `${source.path}:${l.start}-${l.end}` : `${source.path}:${l.start}`
}

/**
 * `draftText` with `quote` appended as a `> ` blockquote, a blank line between it
 * and what was already there, and a blank line after it so the caret lands below.
 * Two quotes in a row are two separate blocks: a blockquote only ends at a blank
 * line. A blank `quote` leaves the draft untouched.
 */
export function appendQuote(draftText: string, quote: string, source?: QuoteSource): string {
  // Leading/trailing blank lines would quote as empty `>` lines.
  const body = quote.replace(/\r\n?/g, '\n').replace(/^\s*\n|\s+$/g, '')
  if (!body.trim()) return draftText
  const lines = source ? [`\`${sourceLabel(source)}\``, ...body.split('\n')] : body.split('\n')
  const block = lines.map((l) => (l.trim() ? `> ${l}` : '>')).join('\n')
  const head = draftText.replace(/\s+$/, '')
  return head ? `${head}\n\n${block}\n\n` : `${block}\n\n`
}

/** A run, as far as picking where a quote goes. */
export interface QuoteRunCandidate {
  id: string
  conductor_run_id?: string | null
}

/**
 * The session a quote from a file window goes to: the run that opened the file
 * while it is still in `runs`, else the project's most recently active session.
 * Workers are skipped there (they sit under their conductor). Null when the
 * project has no session, so Quote is hidden.
 */
export function pickQuoteRun<T extends QuoteRunCandidate>(
  runs: readonly T[],
  originRunId: string | null | undefined,
  activityAt: (run: T) => string,
): T | null {
  const origin = originRunId ? runs.find((r) => r.id === originRunId) : undefined
  if (origin) return origin
  let latest: T | null = null
  for (const r of runs) {
    if (r.conductor_run_id) continue
    if (!latest || activityAt(r) > activityAt(latest)) latest = r
  }
  return latest
}
