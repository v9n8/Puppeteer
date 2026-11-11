// Quote → composer (the shared selection toolbar's Quote button, the row under a
// chat turn, and a file window through lib/quoteAppend.ts).
//
// A quote is structured data in the draft (UX-29): a card above the composer
// that knows where it came from (`DraftQuote`). Only when the message is sent
// does it become text: a markdown blockquote ahead of what the user typed
// (`composeWithQuotes`), byte for byte what `appendQuote` used to leave in the
// draft, so the model sees the same thing as before. The send also saves the
// quotes (`runs::quotes` in Rust), so the sent message shows them as numbered
// cards (`RunQuote`) that lead back to the quoted text. Kept free of Vue /
// Tauri / `@/` imports so it can be unit-tested with `node --test`.

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

// ── Structured quotes (UX-29) ───────────────────────────────────────────────

interface QuoteBase {
  /** Local to the draft: picks the card to remove / annotate. */
  id: string
  /** The quoted text, as the model gets it. */
  excerpt: string
  /** The user's remark on it, sent right below the blockquote. */
  note?: string
}

/**
 * A quote of an assistant message of the session, anchored by the message's
 * id (`StreamEntry.mid`), never by its text: the same phrase quoted twice still
 * leads back to the occurrence that was selected. `start` / `end` are UTF-16
 * offsets into the text that block renders as (lib/quoteAnchor); equal offsets
 * mean the whole message (the Quote button under a turn).
 */
export interface EntryQuote extends QuoteBase {
  kind: 'entry'
  run_id: string
  source_entry_id: string
  block_index: number
  start: number
  end: number
}

/** A selection in a file window (D1-06). */
export interface FileQuote extends QuoteBase {
  kind: 'file'
  path: string
  lines?: { start: number; end: number }
}

/** Nothing to go back to: a question bubble, a message without an id. */
export interface TextQuote extends QuoteBase {
  kind: 'text'
}

export type DraftQuote = EntryQuote | FileQuote | TextQuote

/** Where a selection or a turn sits in the session's log (lib/quoteAnchor). */
export interface QuoteAnchorRef {
  entryId: string
  block: number
  start: number
  end: number
}

// A blank note is no note: the card's note field stays empty.
const noteField = (note: string | undefined) => (note?.trim() ? { note } : {})

/** A new quote for the draft: anchored to a message of `runId`, or plain; `note` typed in the Quote popover. */
export function entryOrTextQuote(
  id: string,
  excerpt: string,
  runId: string | null,
  anchor: QuoteAnchorRef | null | undefined,
  note?: string,
): DraftQuote {
  if (runId && anchor && anchor.entryId) {
    return {
      id, kind: 'entry', excerpt, run_id: runId, source_entry_id: anchor.entryId,
      block_index: anchor.block, start: anchor.start, end: anchor.end, ...noteField(note),
    }
  }
  return { id, kind: 'text', excerpt, ...noteField(note) }
}

export function fileQuote(id: string, excerpt: string, source: QuoteSource, note?: string): FileQuote {
  return {
    id, kind: 'file', excerpt, path: source.path, ...(source.lines ? { lines: { ...source.lines } } : {}), ...noteField(note),
  }
}

/** The fields the blockquote is written from. */
type SerializableQuote = Pick<DraftQuote, 'excerpt' | 'note'> & { path?: string; lines?: QuoteSource['lines'] }

/**
 * The quotes as they go to the model: one blockquote each (a file quote's
 * source as its first line), its note as a paragraph below it, each followed
 * by a blank line. Without notes this is exactly what `appendQuote` built up
 * in the draft before UX-29.
 */
export function serializeQuotes(quotes: readonly SerializableQuote[]): string {
  let out = ''
  for (const q of quotes) {
    const next = appendQuote(out, q.excerpt, q.path ? { path: q.path, ...(q.lines ? { lines: q.lines } : {}) } : undefined)
    if (next === out) continue
    out = next
    const note = q.note?.trim()
    if (note) out = `${out}${note}\n\n`
  }
  return out
}

/** The prompt text: the quotes, then what the user typed. Trailing blanks dropped, as the send trims. */
export function composeWithQuotes(quotes: readonly SerializableQuote[], text: string): string {
  const prefix = serializeQuotes(quotes)
  return prefix ? `${prefix}${text}`.replace(/\s+$/, '') : text
}

/**
 * What the user typed in a sent prompt, without the quotes `prefix`
 * (`serializeQuotes`) it starts with. Null when it doesn't start with them
 * (the prompt was changed on the way, e.g. a log rebuilt from a transcript).
 */
export function stripQuotePrefix(text: string, prefix: string): string | null {
  if (!prefix) return text
  if (text.startsWith(prefix)) return text.slice(prefix.length)
  return text === prefix.replace(/\s+$/, '') ? '' : null
}

/** One quote of a sent prompt, as `list_run_quotes` returns it. */
export interface RunQuote {
  id: string
  run_id: string
  /** The prompt it belongs to: `quote_set` on the prompt's log record. */
  quote_set: string
  position: number
  kind: 'entry' | 'file' | 'text'
  source_run_id: string | null
  source_entry_id: string | null
  block_index: number | null
  start_offset: number | null
  end_offset: number | null
  path: string | null
  line_start: number | null
  line_end: number | null
  excerpt: string
  note: string
  created_at: string
  /** Byte offset of the quoted message's record in the run log now; null = gone. */
  source_offset: number | null
  /** Just sent from here, its source on screen: not read back from the log yet. */
  pending?: boolean
}

/** The quotes a draft hands the send (`send_user_message`'s `quotes`). */
export interface QuoteInput {
  kind: DraftQuote['kind']
  source_run_id?: string
  source_entry_id?: string
  block_index?: number
  start_offset?: number
  end_offset?: number
  path?: string
  line_start?: number
  line_end?: number
  excerpt: string
  note: string
}

export function quoteInput(q: DraftQuote): QuoteInput {
  const note = q.note?.trim() ?? ''
  if (q.kind === 'entry') {
    return {
      kind: 'entry', source_run_id: q.run_id, source_entry_id: q.source_entry_id, block_index: q.block_index,
      start_offset: q.start, end_offset: q.end, excerpt: q.excerpt, note,
    }
  }
  if (q.kind === 'file') {
    return {
      kind: 'file', path: q.path, ...(q.lines ? { line_start: q.lines.start, line_end: q.lines.end } : {}),
      excerpt: q.excerpt, note,
    }
  }
  return { kind: 'text', excerpt: q.excerpt, note }
}

/** A sent quote back as a draft quote (fresh `id`). */
export function draftQuoteOf(q: RunQuote, id: string): DraftQuote {
  const base = { id, excerpt: q.excerpt, ...(q.note ? { note: q.note } : {}) }
  if (q.kind === 'entry' && q.source_entry_id) {
    return {
      ...base, kind: 'entry', run_id: q.source_run_id ?? q.run_id, source_entry_id: q.source_entry_id,
      block_index: q.block_index ?? 0, start: q.start_offset ?? 0, end: q.end_offset ?? 0,
    }
  }
  if (q.kind === 'file' && q.path) {
    const lines = q.line_start != null ? { start: q.line_start, end: q.line_end ?? q.line_start } : undefined
    return { ...base, kind: 'file', path: q.path, ...(lines ? { lines } : {}) }
  }
  return { ...base, kind: 'text' }
}

/**
 * A sent prompt back as a draft (UX-30 "Edit and resend"): the text the user
 * typed and its quotes as cards. When the text no longer starts with them, the
 * quotes stay inside the text instead, so nothing is sent twice.
 */
export function draftFromSentPrompt(
  text: string,
  quotes: readonly RunQuote[],
  newId: () => string,
): { text: string; quotes: DraftQuote[] } {
  const ordered = [...quotes].sort((a, b) => a.position - b.position)
  const drafts = ordered.map((q) => draftQuoteOf(q, newId()))
  const typed = stripQuotePrefix(text, serializeQuotes(drafts))
  return typed === null ? { text, quotes: [] } : { text: typed, quotes: drafts }
}

/** A sent quote's card: whether clicking it leads anywhere. */
export interface QuoteCard {
  quote: RunQuote
  /** An entry quote whose message is still in the log, or a file quote. */
  jumpable: boolean
}

/** One prompt's quotes as the message bubble shows them. */
export interface QuoteSetView {
  cards: QuoteCard[]
  /** The blockquote text they went out as: dropped from the bubble's text. */
  prefix: string
}

/** A run's sent quotes, grouped by the prompt that carried them. */
export function quoteSetViews(rows: readonly RunQuote[]): Map<string, QuoteSetView> {
  const sets = new Map<string, RunQuote[]>()
  for (const q of rows) {
    const list = sets.get(q.quote_set)
    if (list) list.push(q)
    else sets.set(q.quote_set, [q])
  }
  const views = new Map<string, QuoteSetView>()
  for (const [set, list] of sets) {
    list.sort((a, b) => a.position - b.position)
    const cards = list.map((quote) => ({
      quote,
      jumpable: quote.kind === 'file'
        ? !!quote.path
        : quote.kind === 'entry' && !!quote.source_entry_id && (quote.source_offset !== null || !!quote.pending),
    }))
    views.set(set, { cards, prefix: serializeQuotes(list.map((q) => draftQuoteOf(q, q.id))) })
  }
  return views
}

/** The quotes of a stored draft or queued message, keeping only well-formed ones. */
export function sanitizeDraftQuotes(raw: unknown): DraftQuote[] {
  if (!Array.isArray(raw)) return []
  const out: DraftQuote[] = []
  const int = (v: unknown): v is number => typeof v === 'number' && Number.isInteger(v) && v >= 0
  for (const r of raw) {
    if (!r || typeof r !== 'object') continue
    const q = r as Record<string, unknown>
    if (typeof q.id !== 'string' || !q.id || typeof q.excerpt !== 'string' || !q.excerpt.trim()) continue
    const base = { id: q.id, excerpt: q.excerpt, ...(typeof q.note === 'string' && q.note ? { note: q.note } : {}) }
    if (q.kind === 'entry' && typeof q.run_id === 'string' && typeof q.source_entry_id === 'string' && q.source_entry_id
      && int(q.block_index) && int(q.start) && int(q.end) && q.end >= q.start) {
      out.push({
        ...base, kind: 'entry', run_id: q.run_id, source_entry_id: q.source_entry_id,
        block_index: q.block_index, start: q.start, end: q.end,
      })
    } else if (q.kind === 'file' && typeof q.path === 'string' && q.path) {
      const l = q.lines as Record<string, unknown> | undefined
      const lines = l && int(l.start) && int(l.end) ? { start: l.start, end: l.end } : undefined
      out.push({ ...base, kind: 'file', path: q.path, ...(lines ? { lines } : {}) })
    } else if (q.kind === 'text') {
      out.push({ ...base, kind: 'text' })
    }
  }
  return out
}

/**
 * The quotes of a prompt just sent from this window, as cards before the log
 * is read back: the source of an entry quote is on screen (it was just
 * selected), so it counts as there.
 */
export function pendingRunQuotes(runId: string, quoteSet: string, quotes: readonly DraftQuote[]): RunQuote[] {
  return quotes.map((q, position) => {
    const i = quoteInput(q)
    return {
      id: q.id, run_id: runId, quote_set: quoteSet, position, kind: i.kind,
      source_run_id: i.source_run_id ?? null, source_entry_id: i.source_entry_id ?? null,
      block_index: i.block_index ?? null, start_offset: i.start_offset ?? null, end_offset: i.end_offset ?? null,
      path: i.path ?? null, line_start: i.line_start ?? null, line_end: i.line_end ?? null,
      excerpt: i.excerpt, note: i.note, created_at: '', source_offset: null, pending: i.kind === 'entry',
    }
  })
}
