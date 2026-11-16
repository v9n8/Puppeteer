// Moving a screen between its two homes: the main window, or its own window
// (lib/windowPlacement.ts keeps the choice). The route guard sends a screen
// whose home is its own window there; "Open in new window" (DetachButton) and
// "Move back to main window" (MergeBackButton) switch the home.
import type { RouteLocationNormalized, Router } from 'vue-router'
import { emit } from '@tauri-apps/api/event'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { getOpenMode, setOpenMode, type DetachableScreen } from '@/lib/windowPlacement'

/**
 * Emitted by a screen's own window: the main window comes forward and opens
 * `path` (it has the router; the pop-out has none). Handled in
 * composables/useMainWindowEvents.ts. Meant to replace the per-window
 * *_WINDOW_OPEN_ROUTE events over time.
 */
export const SCREEN_OPEN_ROUTE = 'screen:open-route'
export interface ScreenOpenRoute {
  path: string
}

export interface ScreenTarget {
  screen: DetachableScreen
  /** The screen's route in the main window, e.g. `/calendar`. */
  path: string
  /** Opens the screen's own window, or brings it forward when it is open. */
  openWindow: () => Promise<unknown>
}

function openWindowLogged(target: ScreenTarget): Promise<unknown> {
  return target.openWindow().catch((e) => {
    console.error(`[detachedScreens] cannot open the ${target.screen} window`, e)
  })
}

/**
 * For `beforeEnter`: a screen whose home is its own window opens (or raises) that
 * window and the main window stays where it was — or lands on /projects on a cold
 * start, when there is nowhere to stay. Otherwise the route renders here.
 */
export function screenGuard(
  target: ScreenTarget,
): (to: RouteLocationNormalized, from: RouteLocationNormalized) => Promise<boolean | string> {
  return async (_to, from) => {
    if (getOpenMode(target.screen) === 'main') return true
    void openWindowLogged(target)
    return from.matched.length > 0 ? false : '/projects'
  }
}

/** "Open in new window" (main window): the screen moves to its own window. */
export async function detachScreen(target: ScreenTarget, router: Router): Promise<void> {
  setOpenMode(target.screen, 'window')
  try {
    await target.openWindow()
  } catch (e) {
    // No window to move to: the screen stays here.
    setOpenMode(target.screen, 'main')
    throw e
  }
  // Leave the screen if the main window shows it: back to the previous screen, or /projects.
  const isScreen = (path: string) => {
    const p = path.split(/[?#]/)[0]
    return p === target.path || p.startsWith(`${target.path}/`)
  }
  if (!isScreen(router.currentRoute.value.path)) return
  const back: unknown = window.history.state?.back
  if (typeof back === 'string' && !isScreen(back)) router.back()
  else await router.replace('/projects').catch(() => {})
}

/** "Move back to main window" (in the screen's own window): the main window opens it and this window closes. */
export async function mergeScreen(target: Pick<ScreenTarget, 'screen' | 'path'>): Promise<void> {
  setOpenMode(target.screen, 'main')
  await emit<ScreenOpenRoute>(SCREEN_OPEN_ROUTE, { path: target.path })
  await getCurrentWebviewWindow().close()
}
