// Opens THE Slack threads window (views/SlackThreadsWindow.vue,
// `?slackWindow=1[&id=][&add=1][&projectId=]`): a Slack-like two-pane reader of
// the threads Puppeteer fetched and translated. There is only ever ONE such window
// (fixed label); opening it again focuses it and re-points it at the requested
// thread / opens the add dialog.
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { emit } from '@tauri-apps/api/event'
import { trackWindowForTray } from '@/lib/tray'
import { placementOptions, trackWindowBounds } from '@/lib/windowBounds'
import { SCREEN_OPEN_ROUTE, type ScreenOpenRoute, type ScreenTarget } from '@/lib/detachedScreens'
import { getOpenMode } from '@/lib/windowPlacement'
import { SLACK_PATH, slackPath } from '@/lib/screenRoutes'
import { popoutLabel, popoutUrl } from '@/lib/popout'
import { awaitWindowCreated } from '@/lib/windowCreateGuard'
import { useFeaturesStore } from '@/stores/features'

/** Window label; src-tauri/capabilities/default.json grants it the app permissions. */
export const SLACK_WINDOW_LABEL = 'slack-threads'

/**
 * Slack has two homes (lib/windowPlacement keeps the choice, the main window by
 * default): the `/slack` screen of the main window, or this window.
 */
export const slackTarget: ScreenTarget = {
  screen: 'slack',
  path: SLACK_PATH,
  openWindow: () => openSlackThreadsWindow(),
}

/** Sent to the already-open window: select this thread. */
export const SLACK_WINDOW_SELECT = 'slack-window:select'
/** Sent to the already-open window: open the "+ Add thread" dialog. */
export const SLACK_WINDOW_ADD = 'slack-window:add'

/**
 * Emitted by the Slack window; the main window focuses itself and navigates
 * (the pop-out has no router of the app) — e.g. to the MCP settings when there
 * is no Slack token. Handled in App.vue.
 */
export const SLACK_WINDOW_OPEN_ROUTE = 'slack-window:open-route'

/**
 * Emitted by the backend when the user clicks the OS notification of a Slack
 * thread job started over the Inbox API (it also focuses the main window).
 * Only the main window listens (App.vue), so the Slack window opens once.
 */
export const SLACK_NOTIFICATION_CLICKED = 'capture-notification-clicked'

/**
 * Emitted by the backend when a Slack thread arrives over the Inbox API
 * (created or re-sent). Only the main window listens (App.vue) and opens the
 * Slack threads window on it.
 */
export const SLACK_API_RECEIVED = 'slack-threads://api-received'

export interface SlackApiReceived {
  id: string
}

export interface SlackNotificationClicked {
  id: string
  kind: 'slack'
}

export interface SlackWindowOpenRoute {
  path: string
}

export interface SlackWindowSelect {
  id: string
}

export interface SlackWindowAdd {
  projectId: string | null
}

export interface OpenSlackWindowOptions {
  /** Thread to select. */
  id?: string
  /** Open the add dialog. */
  add?: boolean
  /** Project to pre-filter the sidebar with (and default the add dialog to). */
  projectId?: string | null
}

/**
 * Open Slack where the user keeps it: the `/slack` screen of the main window
 * (which comes forward) or the Slack window. Works from any webview.
 */
export async function openSlackWindow(
  opts: OpenSlackWindowOptions = {},
): Promise<WebviewWindow | null> {
  if (getOpenMode('slack') === 'window') return openSlackThreadsWindow(opts)
  // Off: nothing opens, whichever entry asked.
  if (!(await useFeaturesStore().isOnAfterLoad('integrations.slack'))) return null
  await emit<ScreenOpenRoute>(SCREEN_OPEN_ROUTE, { path: slackPath({ projectId: opts.projectId, id: opts.id, add: opts.add }) })
  return null
}

/** Open (or focus) THE Slack window, whatever the saved choice says (Open in new window, the route guard). */
export async function openSlackThreadsWindow(
  opts: OpenSlackWindowOptions = {},
): Promise<WebviewWindow | null> {
  // Off: no window, whichever entry asked.
  if (!(await useFeaturesStore().isOnAfterLoad('integrations.slack'))) return null
  const projectId = opts.projectId ?? null

  const existing = await WebviewWindow.getByLabel(popoutLabel(SLACK_WINDOW_LABEL))
  if (existing) {
    try {
      await existing.unminimize()
      await existing.show()
      await existing.setFocus()
      if (opts.id) await emit(SLACK_WINDOW_SELECT, { id: opts.id } satisfies SlackWindowSelect)
      if (opts.add) await emit(SLACK_WINDOW_ADD, { projectId } satisfies SlackWindowAdd)
    } catch {
      /* window may be mid-teardown */
    }
    return existing
  }

  const params = new URLSearchParams({ slackWindow: '1' })
  if (opts.id) params.set('id', opts.id)
  if (opts.add) params.set('add', '1')
  if (projectId) params.set('projectId', projectId)

  const win = new WebviewWindow(popoutLabel(SLACK_WINDOW_LABEL), {
    url: popoutUrl(`index.html?${params.toString()}`),
    title: 'Slack threads — Puppeteer',
    minWidth: 820,
    minHeight: 520,
    // Where it was last left; the first time 1100 × 760 centred on the user's monitor.
    ...(await placementOptions('slack', { width: 1100, height: 760, minWidth: 820, minHeight: 520 })),
  })
  await awaitWindowCreated(win, 'slackWindow')
  trackWindowForTray(win)
  trackWindowBounds(win, 'slack')
  return win
}
