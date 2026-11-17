// Opens THE Settings window (views/SettingsWindow.vue, `?settingsWindow=1`): the
// Settings screen on its own OS window, for users who moved it out of the main
// window ("Open in new window", lib/detachedScreens). There is only ever ONE
// such window (fixed label); opening it again focuses it and switches it to the
// asked section. Mirrors lib/calendarWindow.ts.
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { emit, once, type UnlistenFn } from '@tauri-apps/api/event'
import { trackWindowForTray } from '@/lib/tray'
import { placementOptions, trackWindowBounds } from '@/lib/windowBounds'
import { popoutLabel, popoutUrl } from '@/lib/popout'
import { awaitWindowCreated } from '@/lib/windowCreateGuard'
import { settingsLocation, type SettingsPlace } from '@/lib/settingsSections'

/** Window label; src-tauri/capabilities/default.json grants it (and `remote-settings`) the app permissions. */
export const SETTINGS_WINDOW_LABEL = 'settings'

/** Sent to the Settings window: show this place (a SettingsPlace). */
export const SETTINGS_WINDOW_OPEN_SECTION = 'settings-window:open-section'

/** Emitted by a freshly loaded Settings window once it listens for OPEN_SECTION. */
export const SETTINGS_WINDOW_READY = 'settings-window:ready'

// In-flight creation guard (like lib/calendarWindow.ts): two quick clicks would
// otherwise both pass the getByLabel() null check and create 'settings' twice.
let creating: Promise<WebviewWindow> | null = null
// Place asked for while the window was being created; handed over once it is ready.
let pendingPlace: SettingsPlace | null = null
let unlistenReady: UnlistenFn | null = null

async function focusWindow(win: WebviewWindow) {
  try {
    await win.unminimize()
    await win.show()
    await win.setFocus()
  } catch {
    /* window may be mid-teardown */
  }
}

/**
 * Opens the Settings window on `place`, or brings it forward and switches it
 * there. `null` (plain `/settings`, ⌘,) only brings it forward; a new window
 * then opens on General.
 */
export async function openSettingsWindow(place: SettingsPlace | null): Promise<WebviewWindow> {
  if (!creating) {
    const existing = await WebviewWindow.getByLabel(popoutLabel(SETTINGS_WINDOW_LABEL))
    if (existing) {
      await focusWindow(existing)
      if (place) await emit(SETTINGS_WINDOW_OPEN_SECTION, place).catch(() => {})
      return existing
    }
  }
  // A creation started by another caller (or while we awaited): its ready handshake delivers the place.
  if (creating) {
    if (place) pendingPlace = place
    const win = await creating
    await focusWindow(win)
    return win
  }

  pendingPlace = null
  creating = createSettingsWindow(place ?? { section: 'general' })
  try {
    return await creating
  } finally {
    creating = null
  }
}

async function createSettingsWindow(place: SettingsPlace): Promise<WebviewWindow> {
  // A place asked for during creation can't reach the new webview before it has
  // mounted, so it waits for READY.
  unlistenReady?.()
  unlistenReady = await once(SETTINGS_WINDOW_READY, () => {
    unlistenReady = null
    const next = pendingPlace
    pendingPlace = null
    if (next) void emit(SETTINGS_WINDOW_OPEN_SECTION, next).catch(() => {})
  })

  const loc = settingsLocation(place)
  const query = new URLSearchParams({ settingsWindow: '1', section: place.section, ...loc.query })
  const win = new WebviewWindow(popoutLabel(SETTINGS_WINDOW_LABEL), {
    url: popoutUrl(`index.html?${query.toString()}`),
    title: 'Settings — Puppeteer',
    minWidth: 760,
    minHeight: 520,
    // Where it was last left; the first time 1000 × 720 centred on the current monitor.
    ...(await placementOptions('settings', { width: 1000, height: 720, minWidth: 760, minHeight: 520 })),
  })
  await awaitWindowCreated(win, 'settingsWindow')
  trackWindowForTray(win)
  trackWindowBounds(win, 'settings')
  return win
}
