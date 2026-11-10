// The row of buttons under each chat turn (components/sessions/MsgActions.vue,
// placed by components/StreamLog.vue): where a row goes, what text it acts on,
// which buttons it shows. Kept free of Vue / Tauri / `@/` imports so it can be
// unit-tested with `node --test`.

export type TurnSide = 'question' | 'answer'

export type TurnAction =
  | 'copy' | 'quote' | 'bookmark' | 'fullscreen' | 'fullscreenTurn' | 'saveNote' | 'fork' | 'editResend' | 'rewind'

/** Features a gated button needs on; the others are always offered. */
export const TURN_ACTION_FEATURES = {
  quote: 'text_tools.selection.quote',
  saveNote: 'productivity.notes',
  rewind: 'sessions.rewind',
  editResend: 'sessions.rewind',
} as const

/** A row, keyed in `turnFooters` by the entry it is rendered under. */
export interface TurnFooter {
  side: TurnSide
  /**
   * Entries the row acts on, inclusive: the `user` entry itself for a
   * question, everything after it up to the next `user` entry for an answer.
   */
  from: number
  to: number
  /** There is text to copy / quote / show (an answer may be tool calls only). */
  hasText: boolean
  /**
   * Where "Rewind to here" goes: the snapshot taken before this question, or
   * for an answer the one before the next question (the state the answer left).
   * Null when that prompt carries no snapshot turn or is not loaded.
   */
  rewindTurn: number | null
}

interface EntryLike {
  kind: string
  text?: string
  is_error?: boolean
  turn?: number
}

const turnOf = (e: EntryLike | undefined): number | null =>
  e?.kind === 'user' && typeof e.turn === 'number' ? e.turn : null

// What an answer entry contributes to the turn's text: assistant prose, and a
// result's own text (the parser blanks it when it repeats the last message).
// Tool calls, thinking and failures are not part of the answer.
function answerText(e: EntryLike): string {
  if (e.kind === 'text' || (e.kind === 'result' && !e.is_error)) return e.text?.trim() ? e.text : ''
  return ''
}

/**
 * The turn's answer as the markdown it was rendered from: every answer text in
 * `[from, to]`, one blank line apart.
 */
export function turnAnswerText(entries: readonly EntryLike[], from: number, to: number): string {
  const parts: string[] = []
  for (let i = from; i <= to && i < entries.length; i++) {
    const text = answerText(entries[i])
    if (text) parts.push(text.trim())
  }
  return parts.join('\n\n')
}

/**
 * Where the rows go. A turn runs from a `user` entry to just before the next
 * one (compactions, injected messages and tool calls stay inside it); its
 * question row sits under the `user` entry and its answer row under the turn's
 * last entry. Entries before the first `user` entry are the tail of a turn
 * whose prompt is outside the loaded window, so they get an answer row too.
 * An answer with neither text nor a tool call has no row. While `running`, the
 * last turn is still being written and gets no rows at all.
 */
export function turnFooters(entries: readonly EntryLike[], running: boolean): Map<number, TurnFooter> {
  const footers = new Map<number, TurnFooter>()
  let userAt = -1

  const closeAnswer = (end: number) => {
    const from = userAt + 1
    const to = end - 1
    let hasContent = false
    let hasText = false
    for (let i = from; i <= to; i++) {
      if (answerText(entries[i])) hasText = hasContent = true
      else if (entries[i].kind === 'tool') hasContent = true
    }
    if (hasContent) footers.set(to, { side: 'answer', from, to, hasText, rewindTurn: turnOf(entries[end]) })
  }

  for (let i = 0; i < entries.length; i++) {
    const e = entries[i]
    if (e.kind !== 'user') continue
    closeAnswer(i)
    userAt = i
    footers.set(i, { side: 'question', from: i, to: i, hasText: !!e.text?.trim(), rewindTurn: turnOf(e) })
  }
  if (!running) closeAnswer(entries.length)
  else if (userAt !== -1) footers.delete(userAt)
  return footers
}

/** Entry index of the prompt a row's turn starts with; -1 when it is before the loaded window. */
export function turnPromptAt(f: TurnFooter): number {
  return f.side === 'question' ? f.from : f.from - 1
}

export interface TurnActionContext {
  hasText: boolean
  /** The surface has a session to quote into right now. */
  hasQuoteTarget: boolean
  /** The surface files notes (it declares a capture). */
  canSaveNote: boolean
  /** The surface can rewind to this row's turn now (a snapshot, session stopped). */
  canRewind?: boolean
  /** The surface can fork its session now (a stopped Claude / Codex session). */
  canFork?: boolean
  /** The surface can edit and resend this question now: it rewinds like Rewind (UX-30). */
  canEditResend?: boolean
  /** This side of the turn is bookmarked; null / absent = it cannot be bookmarked here. */
  bookmarked?: boolean | null
}

/**
 * Buttons of a row: the frequent ones inline, the rare ones behind `⋯`. An
 * empty `overflow` means no `⋯` button. Rewind cuts the session, so it comes
 * last, below a divider. Bookmark sits inline on both sides, as in AWOG. Fork
 * is on the answer only: the fork keeps the whole turn. Edit and resend sits
 * inline on the question: clicking only fills the composer, the rewind waits
 * for its Send.
 */
export function turnActions(
  side: TurnSide,
  isOn: (featureId: string) => boolean,
  ctx: TurnActionContext,
): { primary: TurnAction[]; overflow: TurnAction[] } {
  const rewind: TurnAction[] = ctx.canRewind && isOn(TURN_ACTION_FEATURES.rewind) ? ['rewind'] : []
  const bookmark: TurnAction[] = typeof ctx.bookmarked === 'boolean' ? ['bookmark'] : []
  if (side === 'question') {
    const primary: TurnAction[] = ctx.hasText ? ['copy'] : []
    if (ctx.canEditResend && isOn(TURN_ACTION_FEATURES.editResend)) primary.push('editResend')
    if (ctx.hasText) primary.push('fullscreen')
    return { primary: [...primary, ...bookmark], overflow: rewind }
  }
  const primary: TurnAction[] = []
  if (ctx.hasText) {
    primary.push('copy')
    if (ctx.hasQuoteTarget && isOn(TURN_ACTION_FEATURES.quote)) primary.push('quote')
  }
  primary.push(...bookmark)
  if (ctx.hasText) primary.push('fullscreen')
  primary.push('fullscreenTurn')
  const overflow: TurnAction[] =
    ctx.hasText && ctx.canSaveNote && isOn(TURN_ACTION_FEATURES.saveNote) ? ['saveNote'] : []
  if (ctx.canFork) overflow.push('fork')
  return { primary, overflow: [...overflow, ...rewind] }
}
