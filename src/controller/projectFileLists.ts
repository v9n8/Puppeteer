/**
 * The controller's `@`-mention file lists, cached by project path (RT-22, #8).
 *
 * The Host walks the project and pushes the list (`project_file_list`) on every
 * run bind and every new `@`. The controller used to keep one list that each
 * push and each run switch replaced, so coming back to a project showed the
 * other project's list (or none) until the push landed, and an unchanged push
 * still swapped the array. Here each project keeps its list; a push changes it
 * (and its version) only when the files differ (rules in lib/projectFileCache).
 *
 * Pure (no Vue): runStore holds one and copies the shown list into its state.
 */
import { putFileList, type CachedFileList, type FileEntryLike } from '../lib/projectFileCache.ts'

/**
 * The cache key of a run's list: its project's path, or `run:<id>` while the
 * run or project list has not arrived (an older Host, or the first push after
 * pairing comes before them).
 */
export function fileListKey(
  runId: string,
  runs: readonly { id: string; project_id: string }[],
  projects: readonly { id: string; path: string }[],
): string {
  const projectId = runs.find((r) => r.id === runId)?.project_id
  const path = projectId ? projects.find((p) => p.id === projectId)?.path : undefined
  return path || `run:${runId}`
}

export function createPushedFileLists() {
  let lists: Record<string, CachedFileList<FileEntryLike>> = {}
  // The pushed arrays themselves, kept as they came so the composer gets the
  // same array back for as long as the list does not change.
  let files = new Map<string, string[]>()

  return {
    /** Store a pushed list under `key`; true when it differs from the cached one. */
    put(key: string, paths: string[]): boolean {
      const changed = putFileList(lists, key, paths.map((path) => ({ path, is_dir: false })))
      if (changed) files.set(key, paths)
      return changed
    },
    get(key: string): string[] | undefined {
      return files.get(key)
    },
    /** Changes only when the list of `key` changes; 0 before one arrived. */
    version(key: string): number {
      return lists[key]?.version ?? 0
    },
    clear(): void {
      lists = {}
      files = new Map()
    },
  }
}
