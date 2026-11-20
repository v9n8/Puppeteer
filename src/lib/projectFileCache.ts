/**
 * The `@`-mention / file-link list of a project, cached by project PATH (RT-22, #8).
 *
 * Switching project remounts RunView, so a list held by the composer was thrown
 * away and the whole folder walked again (`list_project_files`) on every switch,
 * then the log re-rendered every entry when the list landed (its v-memo keys on
 * `version`). This cache outlives the component: coming back to a project reuses
 * its list, and `version` only moves when the list really changed.
 *
 * Pure (no Vue): the store passes a reactive record as `lists`, tests a plain one.
 */

export interface FileEntryLike {
  path: string
  is_dir: boolean
}

export interface CachedFileList<E extends FileEntryLike> {
  entries: E[]
  /** Unique across all paths, so two projects never share a value. */
  version: number
}

/** Same paths, same kinds, same order. */
export function sameFileList(a: readonly FileEntryLike[], b: readonly FileEntryLike[]): boolean {
  if (a === b) return true
  if (a.length !== b.length) return false
  for (let i = 0; i < a.length; i++) {
    if (a[i].path !== b[i].path || a[i].is_dir !== b[i].is_dir) return false
  }
  return true
}

export interface ProjectFileCache {
  /**
   * Make sure `path`'s list is loaded. Without `force`, a list already loaded is
   * kept as is (no walk). With `force`, the folder is walked again, and the
   * list (and version) change only if what came back differs. Concurrent calls
   * for one path share one walk. Resolves true when the list changed.
   */
  ensure(path: string, force?: boolean): Promise<boolean>
  /** Whether `path` has a loaded list (a failed walk does not count). */
  has(path: string): boolean
}

export function createProjectFileCache<E extends FileEntryLike>(
  lists: Record<string, CachedFileList<E>>,
  load: (path: string) => Promise<E[]>,
): ProjectFileCache {
  const inflight = new Map<string, Promise<boolean>>()
  let counter = 0
  for (const l of Object.values(lists)) counter = Math.max(counter, l.version)

  async function walk(path: string): Promise<boolean> {
    let next: E[]
    try {
      next = await load(path)
    } catch {
      // Keep what we had; nothing cached yet means the next call tries again.
      return false
    }
    const prev = lists[path]
    if (prev && sameFileList(prev.entries, next)) return false
    lists[path] = { entries: next, version: ++counter }
    return true
  }

  function ensure(path: string, force = false): Promise<boolean> {
    if (!path) return Promise.resolve(false)
    const running = inflight.get(path)
    if (running) return running
    if (!force && path in lists) return Promise.resolve(false)
    const p = walk(path).finally(() => inflight.delete(path))
    inflight.set(path, p)
    return p
  }

  return { ensure, has: (path) => path in lists }
}
