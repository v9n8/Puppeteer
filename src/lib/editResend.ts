// "Edit and resend" a sent message (UX-30): its text, quotes and images go
// back into the composer; Send rewinds the session to just before that message
// (runs::rewind, as "Rewind to here") and sends the edit as a new message. The
// composer's own draft waits aside until the edit is sent or cancelled. Kept
// free of Vue / Tauri / `@/` imports so it can be unit-tested with `node --test`.

import { draftFromSentPrompt, type DraftQuote, type RunQuote } from './quote.ts'

interface EditableRun {
  status: string
  engine: string
  role?: string | null
  conductor_run_id?: string | null
}

/**
 * Whether a session offers "Edit and resend" now. It rewinds the conversation,
 * so only where Rewind takes the conversation back: a stopped Claude / Codex
 * session that has run. Not a Conductor worker, whose conductor drives it.
 * The message itself also needs a snapshot to rewind to (the turn row checks).
 */
export function canEditResend(run: EditableRun | null | undefined, busy: { running: boolean; sending: boolean }): boolean {
  if (!run || busy.running || busy.sending) return false
  return (
    (run.engine === 'claude' || run.engine === 'codex') &&
    run.status !== 'running' &&
    run.status !== 'fetched' &&
    run.role !== 'worker' &&
    !run.conductor_run_id
  )
}

interface ImageLike {
  media_type: string
  data: string
}

/** A sent message as its bubble shows it. */
export interface SentMessage {
  /** The prompt text as sent: quotes (blockquotes) first, then what was typed. */
  text: string
  /** Its quotes as saved (`list_run_quotes`), if any. */
  quotes: readonly RunQuote[]
  images?: readonly ImageLike[]
}

/** What the composer holds while editing `sent`: the typed text, the quotes as cards, the images. */
export function editDraftOf(
  sent: SentMessage,
  newId: () => string,
): { text: string; quotes: DraftQuote[]; images: ImageLike[] } {
  const { text, quotes } = draftFromSentPrompt(sent.text, sent.quotes, newId)
  return { text, quotes, images: (sent.images ?? []).map(({ media_type, data }) => ({ media_type, data })) }
}

/** The message being edited, and the draft set aside for it. */
export interface EditSession<D> {
  runId: string
  /** Snapshot turn of the message: the rewind goes back to just before it. */
  turn: number
  /** Its place among the session's messages (1-based); null when unknown. */
  number: number | null
  /** The composer's draft from before the edit, given back when it ends. */
  stash: D
  /** The rewind is done: Send now only sends (a failed send is not rewound twice). */
  rewound: boolean
}

/**
 * Start editing a message. Editing another message of the same session keeps
 * the draft set aside the first time: the composer holds an edit by then.
 */
export function beginEdit<D>(
  current: EditSession<D> | null,
  target: { runId: string; turn: number; number: number | null },
  composerDraft: D,
): EditSession<D> {
  const stash = current && current.runId === target.runId ? current.stash : composerDraft
  return { ...target, stash, rewound: false }
}

/** End an edit (sent or cancelled): the draft to put back into the composer. */
export function endEdit<D>(session: EditSession<D>): D {
  return session.stash
}

/**
 * What Send does with a rewind preview (`preview_rewind`):
 * - 'refuse': the conversation cannot go back (engine, a newer session, the
 *   message not in the log). Resending would only add a message after the old
 *   one, so nothing is rewound or sent.
 * - 'confirm': files would be restored: the Rewind dialog lists them first,
 *   as "Rewind to here" does.
 * - 'rewind': no file is touched: rewind straight away.
 */
export function rewindStep(preview: {
  conversation: boolean
  files: { changes: readonly unknown[]; kept: readonly unknown[] }
}): 'refuse' | 'confirm' | 'rewind' {
  if (!preview.conversation) return 'refuse'
  return preview.files.changes.length > 0 || preview.files.kept.length > 0 ? 'confirm' : 'rewind'
}

/** Refusals of `preview_rewind` / `rewind_run` with a message of their own (`run.rewind.errors.*`). */
export const REWIND_ERRORS = [
  'run_active',
  'folder_busy',
  'files_changed',
  'snapshot_damaged',
  'snapshot_missing',
  'snapshot_unavailable',
  'snapshot_not_found',
  'workspace_missing',
  'too_many_files',
] as const

/** The message key of a rewind refusal; null for any other error (shown as is). */
export function rewindErrorKey(error: unknown): string | null {
  const code = String(error).split(':')[0]
  return (REWIND_ERRORS as readonly string[]).includes(code) ? `run.rewind.errors.${code}` : null
}
