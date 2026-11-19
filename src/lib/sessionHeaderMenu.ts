// What the Session header's ⋯ menu holds, as a list of blocks in order. The main
// window keeps it short (the History row has the session's actions); a session's
// own window has no History column, so its menu carries them too, plus Delete.

export type SessionHeaderMenuBlock =
  | 'openProjectIn'
  | 'runActions'
  | 'viewProject'
  | 'projectSettings'
  | 'delete'

export function sessionHeaderMenu(opts: {
  isSessionWindow: boolean
  hasProject: boolean
}): SessionHeaderMenuBlock[] {
  const project: SessionHeaderMenuBlock[] = opts.hasProject ? ['viewProject', 'projectSettings'] : []
  if (opts.isSessionWindow) return ['openProjectIn', 'runActions', ...project, 'delete']
  return ['openProjectIn', ...project]
}
