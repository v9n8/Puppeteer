/**
 * Long pastes as a file on the remote controller (RT-26, #16). Pure (no Vue):
 * the composer asks {@link pasteChoice}, App.vue runs {@link requestPasteFile}.
 *
 * The Host writes the text with `save_pasted_text` (the same writer the desktop
 * uses) and acks with the file's Host path; the composer attaches that path as a
 * file reference (sent in `mentions`, which the Host backticks into the prompt —
 * the model reads it like the desktop's "Attached files:" list).
 *
 * The Host's run gate can drop the command with only a notice and no ack, so a
 * missing ack after {@link PASTE_ACK_TIMEOUT_MS} is a failure: the caller puts
 * the text back into the draft so nothing pasted is lost.
 */
import { shouldPasteAsFile, type PasteAsFileSetting } from '../lib/pasteAsFile.ts'
import type { CmdPayload } from './protocol.ts'

/** Backstop for an ack that never comes. The connection's own timer (22 s)
 * usually fires first, as `host_ack_timeout`. */
export const PASTE_ACK_TIMEOUT_MS = 25_000

export interface PasteInput {
  /** The paste carried an image (that path wins; the text is left alone). */
  hasImage: boolean
  /** The run on screen; null for a new session not started yet. */
  runId: string | null
  /** The pasted `text/plain`. */
  text: string
  /** The project's setting as `project_list` sent it (absent = default). */
  setting: Partial<PasteAsFileSetting> | null | undefined
}

/** `file`: save on the Host and attach; `text`: let the browser paste it. */
export function pasteChoice(p: PasteInput): 'file' | 'text' {
  if (p.hasImage || !p.runId || !p.text) return 'text'
  return shouldPasteAsFile(p.text, p.setting) ? 'file' : 'text'
}

export function savePastedTextCmd(runId: string, text: string): CmdPayload {
  return { kind: 'cmd', action: 'save_pasted_text', run_id: runId, text }
}

export type PasteSaveResult = { ok: true; path: string } | { ok: false; reason: string }

/** `sendCommand` as the connection exposes it (ack: accepted, reason, run id,
 * item id, path). */
export type PasteSend = (
  cmd: CmdPayload,
  onAck: (accepted: boolean, reason?: string | null, runId?: string | null, itemId?: string | null, path?: string | null) => void,
) => boolean

export interface PasteTimers {
  set: (fn: () => void, ms: number) => unknown
  clear: (handle: unknown) => void
}

const realTimers: PasteTimers = {
  set: (fn, ms) => setTimeout(fn, ms),
  clear: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
}

/**
 * Ask the Host to save `text` for `runId`. Settles once: the ack, the timeout,
 * or `not_sent` when the command could not leave. A late ack is ignored.
 */
export function requestPasteFile(
  send: PasteSend,
  runId: string,
  text: string,
  opts: { timeoutMs?: number; timers?: PasteTimers } = {},
): Promise<PasteSaveResult> {
  const timers = opts.timers ?? realTimers
  return new Promise((resolve) => {
    let done = false
    let handle: unknown = null
    const finish = (r: PasteSaveResult): void => {
      if (done) return
      done = true
      if (handle !== null) timers.clear(handle)
      resolve(r)
    }
    const sent = send(savePastedTextCmd(runId, text), (accepted, reason, _run, _item, path) => {
      if (accepted && path) finish({ ok: true, path })
      else finish({ ok: false, reason: accepted ? 'no_path' : reason || 'rejected' })
    })
    if (!sent) {
      finish({ ok: false, reason: 'not_sent' })
      return
    }
    if (!done) handle = timers.set(() => finish({ ok: false, reason: 'timeout' }), opts.timeoutMs ?? PASTE_ACK_TIMEOUT_MS)
  })
}

/** The i18n key (under `controller.composer`) for a failed save. */
export function pasteErrorKey(reason: string): 'pasteTooLarge' | 'pasteTimeout' | 'pasteNotSent' | 'pasteFailed' {
  switch (reason) {
    case 'too_large':
      return 'pasteTooLarge'
    case 'timeout':
    case 'host_ack_timeout':
      return 'pasteTimeout'
    case 'not_sent':
    case 'connection_lost':
    case 'disconnected':
      return 'pasteNotSent'
    default:
      return 'pasteFailed'
  }
}
