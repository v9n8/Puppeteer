// Pure half of lib/windowPlacement.ts: where each window-able screen opens
// (main window or its own window) and the size / position its own window had
// last. No Tauri, no storage, so `node --test` can load it
// (src/lib/__tests__/windowPlacement.test.mjs); windowPlacement.ts re-exports
// all of it next to the localStorage side.

/** Screens with two homes (main window / own window). A new one = add it here. */
export type DetachableScreen =
  | 'tasks' | 'notes' | 'slack' | 'calendar' | 'gantt' | 'pr-inbox'
  | 'secret-notes' | 'settings' | 'file-transfer'
/** Windows whose size / position is remembered. 'session', 'file': many windows sharing one set. */
export type WindowKind = DetachableScreen | 'session' | 'file'
export type OpenMode = 'main' | 'window'
/** Logical units (CSS px), screen coordinates. */
export interface WindowBounds { x: number; y: number; width: number; height: number }
export interface WindowPlace { mode?: OpenMode; bounds?: WindowBounds }
export type PlacementMap = Partial<Record<WindowKind, WindowPlace>>

export const PLACEMENT_STORAGE_KEY = 'puppeteer.windowPlacement'
// Stored as {"v":1,"local":PlacementMap,"remote":PlacementMap}. "remote" is for the
// windows a remote desktop opens (`remote-…` labels), so they never overwrite
// where this machine's own windows sit.
const PLACEMENT_VERSION = 1

export const DETACHABLE_SCREENS: readonly DetachableScreen[] = [
  'tasks', 'notes', 'slack', 'calendar', 'gantt', 'pr-inbox', 'secret-notes', 'settings', 'file-transfer',
]
const WINDOW_KINDS: readonly WindowKind[] = [...DETACHABLE_SCREENS, 'session', 'file']
const OPEN_MODES: readonly OpenMode[] = ['main', 'window']

// Secret notes and File transfer keep to their own window unless the user merges them (audit §5).
export const DEFAULT_OPEN_MODE: Record<DetachableScreen, OpenMode> = {
  tasks: 'main',
  notes: 'main',
  slack: 'main',
  calendar: 'main',
  gantt: 'main',
  'pr-inbox': 'main',
  'secret-notes': 'window',
  settings: 'main',
  'file-transfer': 'window',
}

/** How much of a window's title bar must stay on a screen for the user to grab it. */
const MIN_VISIBLE = { width: 64, height: 32 }

function isRecord(v: unknown): v is Record<string, unknown> {
  return !!v && typeof v === 'object' && !Array.isArray(v)
}

function isDetachable(kind: WindowKind): kind is DetachableScreen {
  return (DETACHABLE_SCREENS as readonly string[]).includes(kind)
}

function parseBounds(v: unknown): WindowBounds | undefined {
  if (!isRecord(v)) return undefined
  const { x, y, width, height } = v
  const nums = [x, y, width, height]
  if (!nums.every((n) => typeof n === 'number' && Number.isFinite(n))) return undefined
  // x / y may be negative (a screen left of / above the primary one); a size may not.
  if ((width as number) <= 0 || (height as number) <= 0) return undefined
  return {
    x: Math.round(x as number),
    y: Math.round(y as number),
    width: Math.round(width as number),
    height: Math.round(height as number),
  }
}

function parseMap(v: unknown): PlacementMap {
  const map: PlacementMap = {}
  if (!isRecord(v)) return map
  for (const kind of WINDOW_KINDS) {
    const entry = v[kind]
    if (!isRecord(entry)) continue
    const place: WindowPlace = {}
    if (isDetachable(kind) && OPEN_MODES.includes(entry.mode as OpenMode)) place.mode = entry.mode as OpenMode
    const bounds = parseBounds(entry.bounds)
    if (bounds) place.bounds = bounds
    if (place.mode || place.bounds) map[kind] = place
  }
  return map
}

/** Reads the stored value; anything broken, unknown or of another version is dropped. */
export function parsePlacements(raw: string | null): { local: PlacementMap; remote: PlacementMap } {
  let parsed: unknown = null
  try {
    parsed = raw ? JSON.parse(raw) : null
  } catch {
    /* corrupt value — defaults */
  }
  if (!isRecord(parsed) || parsed.v !== PLACEMENT_VERSION) return { local: {}, remote: {} }
  return { local: parseMap(parsed.local), remote: parseMap(parsed.remote) }
}

/** The value to store for both sets. */
export function serializePlacements(all: { local: PlacementMap; remote: PlacementMap }): string {
  return JSON.stringify({ v: PLACEMENT_VERSION, local: all.local, remote: all.remote })
}

export function openModeOf(map: PlacementMap, screen: DetachableScreen): OpenMode {
  return map[screen]?.mode ?? DEFAULT_OPEN_MODE[screen]
}

/** The open mode of every detachable screen (defaults filled in). */
export function openModesOf(map: PlacementMap): Record<DetachableScreen, OpenMode> {
  const modes = { ...DEFAULT_OPEN_MODE }
  for (const screen of DETACHABLE_SCREENS) modes[screen] = openModeOf(map, screen)
  return modes
}

export function withOpenMode(map: PlacementMap, screen: DetachableScreen, mode: OpenMode): PlacementMap {
  return { ...map, [screen]: { ...map[screen], mode } }
}

export function withBounds(map: PlacementMap, kind: WindowKind, bounds: WindowBounds): PlacementMap {
  return { ...map, [kind]: { ...map[kind], bounds: { ...bounds } } }
}

function clamp(n: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(n, hi))
}

/** Size within `monitor`, never below `min` unless the monitor itself is smaller. */
function fitSize(
  size: { width: number; height: number },
  monitor: WindowBounds,
  min: { width: number; height: number },
): { width: number; height: number } {
  return {
    width: Math.round(Math.min(Math.max(size.width, min.width), monitor.width)),
    height: Math.round(Math.min(Math.max(size.height, min.height), monitor.height)),
  }
}

/** Area of the title-bar strip (top MIN_VISIBLE.height px of `b`) that lies on `m`, or 0 if too little to grab. */
function titleBarOverlap(b: WindowBounds, m: WindowBounds): number {
  const w = Math.min(b.x + b.width, m.x + m.width) - Math.max(b.x, m.x)
  const top = b.y
  const h = Math.min(top + MIN_VISIBLE.height, m.y + m.height) - Math.max(top, m.y)
  return w >= MIN_VISIBLE.width && h >= MIN_VISIBLE.height ? w * h : 0
}

/**
 * Place `b` on one of `monitors`: the one holding most of its title bar, shrunk to
 * fit it and moved fully onto it. `null` when no monitor holds enough of the title
 * bar (≥ 64 × 32 px) — e.g. the monitor it was on is unplugged.
 */
export function fitBounds(
  b: WindowBounds,
  monitors: WindowBounds[],
  min: { width: number; height: number },
): WindowBounds | null {
  let best: WindowBounds | null = null
  let bestOverlap = 0
  for (const m of monitors) {
    const overlap = titleBarOverlap(b, m)
    if (overlap > bestOverlap) {
      best = m
      bestOverlap = overlap
    }
  }
  if (!best) return null
  const size = fitSize(b, best, min)
  return {
    x: Math.round(clamp(b.x, best.x, best.x + best.width - size.width)),
    y: Math.round(clamp(b.y, best.y, best.y + best.height - size.height)),
    ...size,
  }
}

/** `size` (shrunk to fit) centred on `monitor`. */
export function centerBounds(
  size: { width: number; height: number },
  monitor: WindowBounds,
  min: { width: number; height: number },
): WindowBounds {
  const fitted = fitSize(size, monitor, min)
  return {
    x: Math.round(monitor.x + (monitor.width - fitted.width) / 2),
    y: Math.round(monitor.y + (monitor.height - fitted.height) / 2),
    ...fitted,
  }
}
