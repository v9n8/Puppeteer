// Opens THE Tasks window (views/FocusWindow.vue, `?focusWindow=1[&view=][&project=][&id=]`):
// the Tasks screen (Focus board + todo list) on its own OS window, so it can sit
// beside a running session or on a second monitor. The label stays `focus`: the
// backend raises and tells that window on a reminder click. There is only ever ONE
// such window; opening it again focuses it and re-points it.
//
// It has two homes (lib/windowPlacement keeps the choice, the main window by
// default): the `/tasks` screen of the main window, or this window.
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { emit } from '@tauri-apps/api/event'
import { trackWindowForTray } from '@/lib/tray'
import { placementOptions, trackWindowBounds } from '@/lib/windowBounds'
import { SCREEN_OPEN_ROUTE, type ScreenOpenRoute, type ScreenTarget } from '@/lib/detachedScreens'
import { getOpenMode } from '@/lib/windowPlacement'
import { popoutLabel, popoutUrl } from '@/lib/popout'
import { awaitWindowCreated } from '@/lib/windowCreateGuard'
import { TASKS_FEATURE } from '@/lib/featureGate'
import { TASKS_PATH, tasksPath, tasksQuery, type TasksLocation } from '@/lib/tasksRoute'
import { useFeaturesStore } from '@/stores/features'

/** Window label; src-tauri/capabilities/default.json grants it the app permissions. */
export const FOCUS_WINDOW_LABEL = 'focus'

/** Sent to the already-open window: re-point it (view, project filter, the todo to show). */
export const TASKS_WINDOW_SELECT = 'tasks-window:select'

export const tasksTarget: ScreenTarget = {
  screen: 'tasks',
  path: TASKS_PATH,
  openWindow: () => openFocusWindow(),
}

// In-flight creation guard (like lib/prInboxWindow.ts): two quick clicks would
// otherwise both pass the getByLabel() null check and create 'focus' twice.
let creating: Promise<WebviewWindow> | null = null

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
 * Where the user keeps the Tasks screen, from any webview (no router needed):
 * its own window, or the main window, which comes forward on `/tasks`.
 */
export async function openTasks(loc: TasksLocation = {}): Promise<void> {
  if (getOpenMode('tasks') === 'window') {
    await openFocusWindow(loc)
    return
  }
  if (!(await useFeaturesStore().isOnAfterLoad(TASKS_FEATURE))) return
  await emit<ScreenOpenRoute>(SCREEN_OPEN_ROUTE, { path: tasksPath(loc) })
}

export async function openFocusWindow(loc: TasksLocation = {}): Promise<WebviewWindow | null> {
  // Off: no window, whichever entry asked.
  if (!(await useFeaturesStore().isOnAfterLoad(TASKS_FEATURE))) return null
  // A creation started by another caller: wait for it, then just focus.
  if (creating) {
    const win = await creating
    await focusWindow(win)
    return win
  }
  const existing = await WebviewWindow.getByLabel(popoutLabel(FOCUS_WINDOW_LABEL))
  if (existing) {
    await focusWindow(existing)
    try {
      await emit(TASKS_WINDOW_SELECT, loc satisfies TasksLocation)
    } catch {
      /* window may be mid-teardown */
    }
    return existing
  }
  // Re-check: another caller may have started creating while we awaited.
  if (creating) {
    const win = await creating
    await focusWindow(win)
    return win
  }

  creating = createFocusWindow(loc)
  try {
    return await creating
  } finally {
    creating = null
  }
}

async function createFocusWindow(loc: TasksLocation): Promise<WebviewWindow> {
  const params = new URLSearchParams({ focusWindow: '1', ...tasksQuery(loc) })
  const win = new WebviewWindow(popoutLabel(FOCUS_WINDOW_LABEL), {
    url: popoutUrl(`index.html?${params.toString()}`),
    title: 'Tasks — Puppeteer',
    minWidth: 720,
    minHeight: 480,
    // Where it was last left; the first time 1100 × 780 centred on the user's monitor.
    ...(await placementOptions('tasks', { width: 1100, height: 780, minWidth: 720, minHeight: 480 })),
  })
  await awaitWindowCreated(win, 'focusWindow')
  trackWindowForTray(win)
  trackWindowBounds(win, 'tasks')
  return win
}
