// Opens THE Calendar window (views/CalendarWindow.vue, `?calendarWindow=1`): the
// Calendar screen on its own OS window, so it can sit beside a running session
// or on a second monitor. There is only ever ONE such window (fixed label);
// opening it again focuses it. Mirrors lib/prInboxWindow.ts.
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { emit, once, type UnlistenFn } from '@tauri-apps/api/event'
import { SCREEN_OPEN_ROUTE, type ScreenOpenRoute, type ScreenTarget } from '@/lib/detachedScreens'
import { getOpenMode } from '@/lib/windowPlacement'
import { useGoogleCalendarStore } from '@/stores/googleCalendar'
import { trackWindowForTray } from '@/lib/tray'
import { placementOptions, trackWindowBounds } from '@/lib/windowBounds'
import type { CalEvent } from '@/stores/googleCalendar'
import { popoutLabel, popoutUrl } from '@/lib/popout'
import { useFeaturesStore } from '@/stores/features'

/** Window label; src-tauri/capabilities/default.json grants it the app permissions. */
export const CALENDAR_WINDOW_LABEL = 'calendar'

/** The screen's route in the main window. */
export const CALENDAR_PATH = '/calendar'

/**
 * The Calendar has two homes (lib/windowPlacement keeps the choice, the main
 * window by default): the `/calendar` screen of the main window, or this window.
 */
export const calendarTarget: ScreenTarget = {
  screen: 'calendar',
  path: CALENDAR_PATH,
  openWindow: () => openCalendarWindow(),
}

/**
 * Emitted by the Calendar window; the main window focuses itself and navigates
 * (the pop-out has no router of the app) — e.g. to /settings for "Reconnect".
 * Handled in App.vue.
 */
export const CALENDAR_WINDOW_OPEN_ROUTE = 'calendar-window:open-route'

/** Sent to the Calendar window: open this event's detail drawer (reminder click). */
export const CALENDAR_WINDOW_OPEN_EVENT = 'calendar-window:open-event'

/** Emitted by a freshly loaded Calendar window once it listens for OPEN_EVENT. */
export const CALENDAR_WINDOW_READY = 'calendar-window:ready'

export interface CalendarWindowOpenRoute {
  path: string
}

export interface OpenCalendarWindowOptions {
  /** Event whose detail drawer to open. */
  event?: CalEvent
}

// In-flight creation guard (like lib/prInboxWindow.ts): two quick clicks would
// otherwise both pass the getByLabel() null check and create 'calendar' twice.
let creating: Promise<WebviewWindow> | null = null
// Event to hand to the window being created once it reports ready.
let pendingEvent: CalEvent | null = null
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
 * Show the Calendar where the user keeps it, optionally on an event (a reminder
 * click). Main window only: it hands the event to its own store.
 */
export async function openCalendar(opts: OpenCalendarWindowOptions = {}): Promise<void> {
  if (getOpenMode('calendar') === 'window') {
    await openCalendarWindow(opts)
    return
  }
  if (!(await useFeaturesStore().isOnAfterLoad('productivity.calendar'))) return
  if (opts.event) useGoogleCalendarStore().requestOpenEvent(opts.event)
  await emit<ScreenOpenRoute>(SCREEN_OPEN_ROUTE, { path: CALENDAR_PATH })
}

export async function openCalendarWindow(
  opts: OpenCalendarWindowOptions = {},
): Promise<WebviewWindow | null> {
  // Off: no window, whichever entry asked.
  if (!(await useFeaturesStore().isOnAfterLoad('productivity.calendar'))) return null
  // A creation started by another caller: its ready handshake delivers the event.
  if (creating) {
    if (opts.event) pendingEvent = opts.event
    const win = await creating
    await focusWindow(win)
    return win
  }
  const existing = await WebviewWindow.getByLabel(popoutLabel(CALENDAR_WINDOW_LABEL))
  if (existing) {
    await focusWindow(existing)
    if (opts.event) await emit(CALENDAR_WINDOW_OPEN_EVENT, opts.event).catch(() => {})
    return existing
  }
  // Re-check: another caller may have started creating while we awaited.
  if (creating) {
    if (opts.event) pendingEvent = opts.event
    const win = await creating
    await focusWindow(win)
    return win
  }

  pendingEvent = opts.event ?? null
  creating = createCalendarWindow()
  try {
    return await creating
  } finally {
    creating = null
  }
}

async function createCalendarWindow(): Promise<WebviewWindow> {
  // The new webview can't receive OPEN_EVENT before it has mounted, so the
  // event waits for its READY.
  unlistenReady?.()
  unlistenReady = await once(CALENDAR_WINDOW_READY, () => {
    unlistenReady = null
    const ev = pendingEvent
    pendingEvent = null
    if (ev) void emit(CALENDAR_WINDOW_OPEN_EVENT, ev).catch(() => {})
  })

  const win = new WebviewWindow(popoutLabel(CALENDAR_WINDOW_LABEL), {
    url: popoutUrl('index.html?calendarWindow=1'),
    title: 'Calendar — Puppeteer',
    minWidth: 760,
    minHeight: 520,
    // Where it was last left; the first time 1200 × 820 centred on the current monitor.
    ...(await placementOptions('calendar', { width: 1200, height: 820, minWidth: 760, minHeight: 520 })),
  })
  win.once('tauri://error', (e) => {
    console.error('[calendarWindow] failed to open window', e)
  })
  trackWindowForTray(win)
  trackWindowBounds(win, 'calendar')
  return win
}
