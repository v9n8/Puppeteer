// RT-06: the backend emits the log line of every prompt it hands the engine
// (`runs::live_records::push_prompt`) to each window following the run, so a
// prompt sent from one window shows in the others. The window that sent it
// already shows it (`liveRuns.pushUser`, marked `echoPending`): there the echo
// only hands its Rewind turn to that entry and adds nothing.

import { applyStreamEvent, createStreamState, type StreamEntry, type StreamState } from './streamEvents'

type UserEntry = Extract<StreamEntry, { kind: 'user' }>

/** The user entry a prompt's log line renders as, or null for any other event. */
function echoEntry(evt: unknown): UserEntry | null {
  if (!evt || typeof evt !== 'object' || (evt as Record<string, unknown>).type !== 'user') return null
  const tmp = createStreamState()
  applyStreamEvent(tmp, evt)
  const only = tmp.entries.length === 1 ? tmp.entries[0] : null
  return only?.kind === 'user' ? only : null
}

/**
 * The oldest entry this window pushed itself that `echo` repeats: same text
 * (trimmed, as the backend logs it), no other turn already given. -1 if none.
 */
export function findEchoedEntry(entries: StreamEntry[], echo: UserEntry): number {
  for (let i = 0; i < entries.length; i++) {
    const e = entries[i]
    if (e.kind !== 'user' || !e.echoPending) continue
    if (e.text.trim() !== echo.text.trim()) continue
    if (e.turn !== undefined && echo.turn !== undefined && e.turn !== echo.turn) continue
    if ((e.images?.length ?? 0) !== (echo.images?.length ?? 0)) continue
    return i
  }
  return -1
}

/**
 * Apply a live `run:event` payload: a prompt this window already shows is
 * dropped (its entry takes the echo's turn); everything else goes to
 * `applyStreamEvent` as before.
 */
export function applyPromptEcho(state: Pick<StreamState, 'entries' | 'toolIndex'>, evt: unknown): void {
  const pending = state.entries.some((e) => e.kind === 'user' && e.echoPending)
  const echo = pending ? echoEntry(evt) : null
  const at = echo ? findEchoedEntry(state.entries, echo) : -1
  if (echo && at >= 0) {
    const mine = state.entries[at] as UserEntry
    delete mine.echoPending
    if (mine.turn === undefined && echo.turn !== undefined) mine.turn = echo.turn
    return
  }
  applyStreamEvent(state, evt)
}
