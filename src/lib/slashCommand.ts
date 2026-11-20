// Slash commands sent from the session UI (e.g. "Compact context" → /compact).
// They go out on their own: the user's draft (text, quotes, images, files) stays
// in the composer, and nothing of it is sent with the command (RT-02, #17).

export type SlashCommandAction = 'send' | 'queue' | 'blocked'

/**
 * What to do with a slash command for a session in `status`:
 * - `queue` while a turn runs (it waits for the turn to end, like a queued message);
 * - `blocked` while a message is being edited (sending rewinds first) or before
 *   the session ever started (`fetched`: the first message becomes its goal);
 * - `send` otherwise (resumes a finished session).
 */
export function slashCommandAction(status: string, editing: boolean): SlashCommandAction {
  if (editing || status === 'fetched') return 'blocked'
  return status === 'running' ? 'queue' : 'send'
}

/** The message for the command alone: no quotes, images or files. */
export function slashCommandMessage(cmd: string): { text: string; images: never[]; filePaths: never[]; quotes: never[] } {
  return { text: cmd, images: [], filePaths: [], quotes: [] }
}
