// What the Session header's ⋯ menu holds, as a list of blocks in order. The main
// window has no such menu (the History row has the session's actions); a session's
// own window has no History column, so its menu carries them, plus Delete.

export type SessionHeaderMenuBlock = 'runActions' | 'delete'

export function sessionHeaderMenu(opts: { isSessionWindow: boolean }): SessionHeaderMenuBlock[] {
  return opts.isSessionWindow ? ['runActions', 'delete'] : []
}
