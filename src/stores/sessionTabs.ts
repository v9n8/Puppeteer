import { defineStore } from 'pinia'
import { ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { FILE_WINDOW_DOCK, type FileWindowDock } from '@/lib/fileWindow'
import {
  closeOthers as closeOthersIn,
  closeTab,
  emptyState,
  openTab,
  parse,
  prune,
  renameTab,
  selectTab,
  serialize,
  togglePin as togglePinIn,
  type SessionTabId,
  type SessionTabsState,
} from '@/lib/sessionTabs'

/**
 * The Session area's tabs, per Session (V3-24; rules in lib/sessionTabs). Kept
 * per machine in localStorage, so they come back after a restart.
 *
 * The main window and a session pop-out each have their own copy. A write
 * therefore re-reads storage and replaces only the Sessions this window changed,
 * and a window re-reads the rest when it gets focus — so the copy of a Session
 * shown in the other window is never written back over it.
 */
const STORAGE_KEY = 'puppeteer.sessionTabs.v1'
const SAVE_DELAY_MS = 300

function readStorage(): Record<string, SessionTabsState> {
  try {
    return parse(localStorage.getItem(STORAGE_KEY))
  } catch {
    return {}
  }
}

function windowLabel(): string | null {
  try {
    return getCurrentWebviewWindow().label
  } catch {
    return null
  }
}

export const useSessionTabsStore = defineStore('sessionTabs', () => {
  const byRun = ref<Record<string, SessionTabsState>>(readStorage())
  const dirty = new Set<string>()
  let saveTimer: ReturnType<typeof setTimeout> | null = null

  function flush() {
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = null
    if (!dirty.size) return
    const merged = readStorage()
    for (const runId of dirty) {
      const state = byRun.value[runId]
      if (state) merged[runId] = state
      else delete merged[runId]
    }
    dirty.clear()
    try {
      localStorage.setItem(STORAGE_KEY, serialize(prune(merged, null, Date.now())))
    } catch (e) {
      console.warn('[sessionTabs] could not save the Session tabs', e)
    }
  }

  /** Take what the other window saved, for every Session this one has not changed since. */
  function refresh() {
    const stored = readStorage()
    const next: Record<string, SessionTabsState> = {}
    for (const runId of new Set([...Object.keys(stored), ...dirty])) {
      const state = dirty.has(runId) ? byRun.value[runId] : stored[runId]
      if (state) next[runId] = state
    }
    byRun.value = next
  }

  function put(runId: string, state: SessionTabsState) {
    if (byRun.value[runId] === state) return
    byRun.value = { ...byRun.value, [runId]: state }
    dirty.add(runId)
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = setTimeout(flush, SAVE_DELAY_MS)
  }

  function stateFor(runId: string): SessionTabsState {
    return byRun.value[runId] ?? emptyState()
  }

  function open(runId: string, path: string, line: number | null) {
    if (!runId || !path) return
    put(runId, openTab(stateFor(runId), path, line, Date.now()))
  }

  function select(runId: string, id: SessionTabId) {
    put(runId, selectTab(stateFor(runId), id, Date.now()))
  }

  function close(runId: string, id: SessionTabId) {
    put(runId, closeTab(stateFor(runId), id))
  }

  function closeOthers(runId: string, id: SessionTabId) {
    put(runId, closeOthersIn(stateFor(runId), id))
  }

  function togglePin(runId: string, id: SessionTabId) {
    put(runId, togglePinIn(stateFor(runId), id))
  }

  function rename(runId: string, oldPath: string, newPath: string) {
    put(runId, renameTab(stateFor(runId), oldPath, newPath))
  }

  /** Deleted Sessions: nothing of theirs to keep. */
  function forget(runIds: Iterable<string>) {
    const next = { ...byRun.value }
    let changed = false
    for (const runId of runIds) {
      if (!(runId in next)) continue
      delete next[runId]
      dirty.add(runId)
      changed = true
    }
    if (!changed) return
    byRun.value = next
    flush()
  }

  // Once per webview: the other window's saves, the last pending write before
  // the window goes, and a file window docking back into one of our Sessions.
  try {
    window.addEventListener('focus', () => { if (!saveTimer) refresh() })
    window.addEventListener('beforeunload', flush)
  } catch {
    /* no window (tests) */
  }
  const me = windowLabel()
  listen<FileWindowDock>(FILE_WINDOW_DOCK, ({ payload: d }) => {
    if (!d?.runId || !d.path || d.writer !== me) return
    // The Session may have been changed in a window that has since closed.
    if (!dirty.has(d.runId)) refresh()
    open(d.runId, d.path, d.line ?? null)
  }).catch(() => {
    /* running outside the Tauri shell */
  })

  return { byRun, stateFor, open, select, close, closeOthers, togglePin, rename, forget, flush }
})
