// Where a Session works on disk, and what the "reveal it" menu item is called on
// this OS. Pure, so the row menu's rules are testable (V3-23).

export type Platform = 'mac' | 'windows' | 'linux'

/** The folder a Session runs in: a worker's own worktree first, else the Project's. */
export function sessionWorkDir(
  run: { worktree_path?: string | null },
  projectPath: string | null | undefined,
): string | null {
  return run.worktree_path || projectPath || null
}

export function platformOf(userAgent: string): Platform {
  if (/mac/i.test(userAgent)) return 'mac'
  if (/win/i.test(userAgent)) return 'windows'
  return 'linux'
}

/** i18n key of the "show this folder in the file manager" item for a platform. */
export function revealLabelKey(platform: Platform): string {
  return `run.revealDir.${platform}`
}
