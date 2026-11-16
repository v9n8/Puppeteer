// Tauri side of lib/windowPlacement.ts: open a window where it was last left,
// and remember where the user moves / resizes it. Used by the lib/*Window.ts
// helpers of the screens whose own window keeps its place.
//
// Units: everything stored is logical (CSS) px. The window reports physical px,
// so they are divided by the window's OWN scale factor — otherwise a window on a
// Retina screen would come back at twice its position and size.
import { availableMonitors } from '@tauri-apps/api/window'
import type { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { centerOnCurrentMonitor, currentMonitorBounds } from '@/lib/windowCascade'
import {
  centerBounds,
  fitBounds,
  getSavedBounds,
  saveBounds,
  type WindowBounds,
  type WindowKind,
} from '@/lib/windowPlacement'

/** Moves and resizes fire every frame while dragging; store once things settle. */
const SAVE_DELAY_MS = 300

/** Logical bounds of every connected monitor; empty when they can't be read. */
async function monitorBounds(): Promise<WindowBounds[]> {
  try {
    return (await availableMonitors()).map((m) => {
      const pos = m.position.toLogical(m.scaleFactor)
      const size = m.size.toLogical(m.scaleFactor)
      return { x: pos.x, y: pos.y, width: size.width, height: size.height }
    })
  } catch {
    return []
  }
}

/**
 * Spread into the options of `new WebviewWindow`: the saved bounds (moved onto a
 * connected monitor), else the default size centred on the current monitor.
 */
export async function placementOptions(
  kind: WindowKind,
  def: { width: number; height: number; minWidth?: number; minHeight?: number },
): Promise<{ width: number; height: number; x?: number; y?: number }> {
  const saved = getSavedBounds(kind)
  if (!saved) {
    return { width: def.width, height: def.height, ...(await centerOnCurrentMonitor(def.width, def.height)) }
  }
  const min = { width: def.minWidth ?? 0, height: def.minHeight ?? 0 }
  const fitted = fitBounds(saved, await monitorBounds(), min)
  if (fitted) return fitted
  // Its monitor is gone (or unreadable): the saved size, centred where the user is.
  const current = await currentMonitorBounds()
  if (current) return centerBounds(saved, current, min)
  return { width: saved.width, height: saved.height }
}

async function readBounds(win: WebviewWindow): Promise<WindowBounds | null> {
  // A maximised / full-screen / minimised window is not the size to come back at.
  if ((await win.isMaximized()) || (await win.isFullscreen()) || (await win.isMinimized())) return null
  const factor = await win.scaleFactor()
  const pos = (await win.outerPosition()).toLogical(factor)
  // `width` / `height` of the window options are the inner size.
  const size = (await win.innerSize()).toLogical(factor)
  const bounds = {
    x: Math.round(pos.x),
    y: Math.round(pos.y),
    width: Math.round(size.width),
    height: Math.round(size.height),
  }
  // Off every screen (Windows parks a minimised window far off-screen): not a real place.
  const monitors = await monitorBounds()
  if (monitors.length > 0 && !fitBounds(bounds, monitors, { width: 0, height: 0 })) return null
  return bounds
}

/** Save `win`'s bounds under `kind` whenever it is moved or resized (debounced), until it closes. */
export function trackWindowBounds(win: WebviewWindow, kind: WindowKind): void {
  let timer: ReturnType<typeof setTimeout> | null = null
  let closed = false
  const unlisteners: UnlistenFn[] = []

  const capture = async () => {
    timer = null
    try {
      const bounds = await readBounds(win)
      if (bounds && !closed) saveBounds(kind, bounds)
    } catch {
      /* window closed mid-read: its last saved bounds stand */
    }
  }
  const schedule = () => {
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => void capture(), SAVE_DELAY_MS)
  }
  const stop = () => {
    closed = true
    if (timer) clearTimeout(timer)
    timer = null
    for (const off of unlisteners.splice(0)) off()
  }

  void win.once('tauri://destroyed', stop)
  for (const attach of [() => win.onMoved(schedule), () => win.onResized(schedule)]) {
    attach()
      .then((off) => (closed ? off() : unlisteners.push(off)))
      .catch((e) => console.error(`[windowBounds] cannot track ${kind} window`, e))
  }
}
