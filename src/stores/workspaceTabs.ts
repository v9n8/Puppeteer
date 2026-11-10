import { defineStore } from 'pinia'
import { reactive, ref } from 'vue'
import { touchMru } from '@/lib/projectSwitcher'

/**
 * A project pinned as a tab in the workspace. Tabs exist purely to switch
 * quickly between PROJECTS — switching between runs inside a project is still
 * done from RunView's History list, not tabs. Each tab remembers the last run
 * the user was viewing in that project so re-selecting the tab returns there.
 */
export interface WorkspaceTab {
  projectId: string
  lastRunId: string | null
}

const STORAGE_KEY = 'puppeteer.workspaceTabs'
// Recently used projects, most recent first: the switcher's "Recent" list and
// the project ⌘` goes back to (UX-09). Kept apart from the tabs, which are
// ordered by hand.
const MRU_STORAGE_KEY = 'puppeteer.recentProjects'

function load(): WorkspaceTab[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    const arr = raw ? JSON.parse(raw) : []
    return Array.isArray(arr)
      ? arr
          .filter((t): t is { projectId: string; lastRunId?: unknown } => !!t && typeof t.projectId === 'string')
          .map((t) => ({ projectId: t.projectId, lastRunId: typeof t.lastRunId === 'string' ? t.lastRunId : null }))
      : []
  } catch {
    return []
  }
}

function loadMru(): string[] {
  try {
    const raw = localStorage.getItem(MRU_STORAGE_KEY)
    const arr = raw ? JSON.parse(raw) : []
    return Array.isArray(arr) ? arr.filter((id): id is string => typeof id === 'string' && !!id) : []
  } catch {
    return []
  }
}

export const useWorkspaceTabsStore = defineStore('workspaceTabs', () => {
  const tabs = reactive<WorkspaceTab[]>(load())
  const recent = ref<string[]>(loadMru())

  function persist() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify([...tabs]))
    } catch {
      /* storage unavailable — in-memory tabs still work this session */
    }
  }

  /**
   * Pin a project as a tab (idempotent) and, when a run is given, record it as
   * the project's last-viewed run so the tab resumes there next time.
   */
  function open(projectId: string, runId?: string | null) {
    if (!projectId) return
    let tab = tabs.find((t) => t.projectId === projectId)
    if (!tab) {
      tab = { projectId, lastRunId: runId ?? null }
      tabs.push(tab)
      persist()
      return
    }
    if (runId && tab.lastRunId !== runId) {
      tab.lastRunId = runId
      persist()
    }
  }

  /**
   * Close a project tab. Returns the neighbouring tab to navigate to (the one
   * that slides into its place, else the previous one), or null if none remain.
   */
  function close(projectId: string): WorkspaceTab | null {
    const idx = tabs.findIndex((t) => t.projectId === projectId)
    if (idx === -1) return null
    tabs.splice(idx, 1)
    persist()
    return tabs[idx] ?? tabs[idx - 1] ?? null
  }

  function has(projectId: string): boolean {
    return tabs.some((t) => t.projectId === projectId)
  }

  /**
   * Move the tab at `from` to `to` (drag-and-drop reorder). Called repeatedly
   * while a drag is in flight, so the tab slides under the cursor live.
   */
  function move(from: number, to: number) {
    if (from === to) return
    if (from < 0 || from >= tabs.length) return
    if (to < 0 || to >= tabs.length) return
    const [moved] = tabs.splice(from, 1)
    tabs.splice(to, 0, moved)
    persist()
  }

  /** Forget a deleted run so a tab never resumes onto a run that's gone. */
  function forgetRun(runId: string) {
    let changed = false
    for (const t of tabs) {
      if (t.lastRunId === runId) {
        t.lastRunId = null
        changed = true
      }
    }
    if (changed) persist()
  }

  /** Record `projectId` as the project in use now (front of the recent list). */
  function touch(projectId: string) {
    if (!projectId || recent.value[0] === projectId) return
    recent.value = touchMru(recent.value, projectId)
    try {
      localStorage.setItem(MRU_STORAGE_KEY, JSON.stringify(recent.value))
    } catch {
      /* storage unavailable — the in-memory list still works this session */
    }
  }

  return { tabs, recent, open, close, has, move, forgetRun, touch }
})
