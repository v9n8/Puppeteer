import { defineStore } from 'pinia'
import { shallowReactive } from 'vue'
import { createProjectFileCache, type CachedFileList } from '@/lib/projectFileCache'
import { useRunsStore, type ProjectEntry } from './runs'

const EMPTY: ProjectEntry[] = []

/**
 * Project file lists keyed by project path (RT-22, #8), shared by every
 * RunView mount of this window so switching project does not walk the folder
 * again. The rules live in lib/projectFileCache.
 *
 * Shallow: each list is replaced as a whole, so the (large) entry arrays are
 * never deep-proxied.
 */
export const useProjectFilesStore = defineStore('projectFiles', () => {
  const lists = shallowReactive<Record<string, CachedFileList<ProjectEntry>>>({})
  const cache = createProjectFileCache<ProjectEntry>(lists, (path) => useRunsStore().listProjectFiles(path))

  function entries(path: string | null | undefined): ProjectEntry[] {
    return (path && lists[path]?.entries) || EMPTY
  }

  /** Changes only when the list of `path` changes; 0 before it is loaded. */
  function version(path: string | null | undefined): number {
    return (path && lists[path]?.version) || 0
  }

  return { entries, version, ensure: cache.ensure }
})
