// Where each window-able screen opens and where its own window sat, kept per
// machine in localStorage (not in settings: a window position means nothing on
// another computer). The pure rules live in ./windowPlacementCore.ts; this file
// adds the storage, split by window set: a remote-desktop window and the
// pop-outs it opens read and write the "remote" set, everything else "local".
// Reads / writes are wrapped like lib/appFonts.ts: blocked or broken storage
// means defaults, never an error.
import { isRemoteMode } from '@/lib/ipc/remote'
import {
  PLACEMENT_STORAGE_KEY,
  openModeOf,
  parsePlacements,
  serializePlacements,
  withBounds,
  withOpenMode,
  type DetachableScreen,
  type OpenMode,
  type PlacementMap,
  type WindowBounds,
  type WindowKind,
} from './windowPlacementCore'

export * from './windowPlacementCore'

type Placements = { local: PlacementMap; remote: PlacementMap }

function side(): keyof Placements {
  return isRemoteMode() ? 'remote' : 'local'
}

function readAll(): Placements {
  try {
    return parsePlacements(localStorage.getItem(PLACEMENT_STORAGE_KEY))
  } catch {
    return { local: {}, remote: {} }
  }
}

// The 'storage' event only reaches the OTHER windows; listeners in this one hear it from here.
const localListeners = new Set<(map: PlacementMap) => void>()

function update(change: (map: PlacementMap) => PlacementMap): void {
  const all = readAll()
  const key = side()
  all[key] = change(all[key])
  try {
    localStorage.setItem(PLACEMENT_STORAGE_KEY, serializePlacements(all))
  } catch {
    /* storage unavailable — kept for this window only */
  }
  for (const cb of localListeners) cb(all[key])
}

export function loadPlacements(): PlacementMap {
  return readAll()[side()]
}

export function getOpenMode(screen: DetachableScreen): OpenMode {
  return openModeOf(loadPlacements(), screen)
}

export function setOpenMode(screen: DetachableScreen, mode: OpenMode): void {
  update((map) => withOpenMode(map, screen, mode))
}

export function getSavedBounds(kind: WindowKind): WindowBounds | undefined {
  return loadPlacements()[kind]?.bounds
}

export function saveBounds(kind: WindowKind, bounds: WindowBounds): void {
  update((map) => withBounds(map, kind, bounds))
}

/** Calls back when any window changes the data (the 'storage' event). Returns the unsubscribe. */
export function onPlacementsChanged(cb: (map: PlacementMap) => void): () => void {
  const onStorage = (e: StorageEvent) => {
    // `key` is null when the whole storage was cleared.
    if (e.key === PLACEMENT_STORAGE_KEY || e.key === null) cb(parsePlacements(e.newValue)[side()])
  }
  localListeners.add(cb)
  window.addEventListener('storage', onStorage)
  return () => {
    localListeners.delete(cb)
    window.removeEventListener('storage', onStorage)
  }
}
