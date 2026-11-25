// Where a Session works on disk (and where its file viewer reads), and what the
// "reveal it" menu item is called on this OS. Pure, so the rules are testable (V3-23).

export type Platform = 'mac' | 'windows' | 'linux'

/** The folder a Session runs in: a worker's own worktree first, else the Project's. */
export function sessionWorkDir(
  run: { worktree_path?: string | null },
  projectPath: string | null | undefined,
): string | null {
  return run.worktree_path || projectPath || null
}

/**
 * The folder a Session's file viewer reads relative paths from: its work dir,
 * or the Project's once the worker's worktree is gone (cleaned up after a
 * cancel or delete), so its files still open instead of all failing.
 */
export function sessionFileRoot(
  run: { worktree_path?: string | null },
  projectPath: string | null | undefined,
  worktreeExists: boolean,
): string | null {
  return worktreeExists ? sessionWorkDir(run, projectPath) : projectPath || null
}

/**
 * A Files tree path (relative to `treeProject`) as a viewer reading from `root`
 * names it: as is when the tree lists `root`, the absolute path when the tree
 * is the Project of a Session working elsewhere (a worktree), null for another
 * Project's tree.
 */
export function treePathIn(treeProject: string, path: string, root: string, projectPath: string): string | null {
  if (treeProject === root) return path
  if (treeProject === projectPath) return `${projectPath.replace(/\/+$/, '')}/${path}`
  return null
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
