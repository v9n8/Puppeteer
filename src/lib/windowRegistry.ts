// What the windows of this machine tell each other through localStorage (it is
// shared by every window of the app, and the 'storage' event wakes the others):
// who opened which window (project / run), which windows were used last, and the
// badge of each run (written by the main window, the only one that sees every
// live run). The pure parts live in ./windowsOverlay.ts. Reads / writes are
// wrapped like lib/windowPlacement.ts: blocked storage means "nothing known".
import {
  META_STORAGE_KEY,
  RECENT_STORAGE_KEY,
  RUN_STATUS_STORAGE_KEY,
  parseMeta,
  parseRecent,
  parseRunStatuses,
  touchRecent,
  type WindowMeta,
  type WindowMetaMap,
  type WindowRunStatus,
} from './windowsOverlay'

function read(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

function write(key: string, value: string): void {
  try {
    localStorage.setItem(key, value)
  } catch {
    /* storage unavailable: the overlay just knows less */
  }
}

export function loadWindowMeta(): WindowMetaMap {
  return parseMeta(read(META_STORAGE_KEY))
}

/** Called by the window that opens another one, with what only it knows. */
export function recordWindow(label: string, meta: WindowMeta | undefined): void {
  if (!meta) return
  write(META_STORAGE_KEY, JSON.stringify({ ...loadWindowMeta(), [label]: meta }))
}

export function forgetWindow(label: string): void {
  const all = loadWindowMeta()
  if (!(label in all)) return
  delete all[label]
  write(META_STORAGE_KEY, JSON.stringify(all))
}

/** Drops the entries of windows that are gone (a window closed while its opener was too). */
export function pruneWindowMeta(openLabels: readonly string[]): void {
  const all = loadWindowMeta()
  const keep = Object.fromEntries(Object.entries(all).filter(([label]) => openLabels.includes(label)))
  if (Object.keys(keep).length !== Object.keys(all).length) write(META_STORAGE_KEY, JSON.stringify(keep))
}

export function loadRecentWindows(): string[] {
  return parseRecent(read(RECENT_STORAGE_KEY))
}

/** This window was used now: it goes first in the "used last" list. */
export function markWindowUsed(label: string): void {
  write(RECENT_STORAGE_KEY, JSON.stringify(touchRecent(loadRecentWindows(), label)))
}

export function loadRunStatuses(): Record<string, WindowRunStatus> {
  return parseRunStatuses(read(RUN_STATUS_STORAGE_KEY))
}

/** Main window only. Skips the write when nothing changed, so the other windows are not woken for nothing. */
export function saveRunStatuses(statuses: Record<string, WindowRunStatus>): void {
  const next = JSON.stringify(statuses)
  if (next !== JSON.stringify(loadRunStatuses())) write(RUN_STATUS_STORAGE_KEY, next)
}
