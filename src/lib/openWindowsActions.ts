// What the "Open windows" overlay does to other windows, and how its command
// finds the window to open on. The pure rules are in ./windowsOverlay.ts.
import { emit } from '@tauri-apps/api/event'
import { WebviewWindow, getAllWebviewWindows } from '@tauri-apps/api/webviewWindow'
import { useOpenWindowsStore } from '@/stores/openWindows'
import { WINDOWS_OVERLAY_OPEN, WINDOW_MERGE_REQUEST, kindOfLabel, type WindowTargeted } from '@/lib/windowsOverlay'

/** Raise a window: out of the Dock, shown, focused. False when it is mid-teardown. */
export async function raiseWindow(label: string): Promise<boolean> {
  const win = await WebviewWindow.getByLabel(label)
  if (!win) return false
  try {
    await win.unminimize()
    await win.show()
    await win.setFocus()
    return true
  } catch {
    return false
  }
}

/** Close like the OS close button: a window with unsaved changes still asks first. */
export async function closeWindow(label: string): Promise<void> {
  await (await WebviewWindow.getByLabel(label))?.close()
}

export async function minimizeWindow(label: string): Promise<void> {
  await (await WebviewWindow.getByLabel(label))?.minimize()
}

/** The window runs its own "Move back to main window" (the WindowsOverlay of that window listens). */
export async function requestMergeBack(label: string): Promise<void> {
  await emit(WINDOW_MERGE_REQUEST, { target: label } satisfies WindowTargeted)
}

/**
 * The Open-windows command (menu item, tray row, ⌘K row, ⌘⇧E on the main window):
 * the overlay opens on the window that has the focus. A private window is asked
 * through an event; the main window, or nothing focused (as when the tray was
 * clicked), gets it here after coming to the front.
 */
export async function openOverlayOnFocusedWindow(): Promise<void> {
  let target = 'main'
  try {
    for (const w of await getAllWebviewWindows()) {
      if (w.label !== 'main' && kindOfLabel(w.label) && (await w.isFocused())) {
        target = w.label
        break
      }
    }
  } catch {
    /* outside the Tauri shell */
  }
  if (target !== 'main') {
    await emit(WINDOWS_OVERLAY_OPEN, { target } satisfies WindowTargeted)
    return
  }
  await raiseWindow('main')
  useOpenWindowsStore().openOrCycle()
}
