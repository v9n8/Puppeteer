// The main window's app-wide event listeners: tray rows, pop-outs asking to
// open a run / a screen / a new session, backend-reported active runs, the
// Trash. Only the main window attaches them (App.vue, past the pop-out and
// session-window early returns), so no other window double-handles an event.
import { onBeforeUnmount } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { useProjectsStore } from '@/stores/projects'
import { openFocusWindow } from '@/lib/focusWindow'
import { FOCUS_OPEN_WINDOW, type FocusOpenTodo } from '@/lib/focus'
import { useLiveRunsStore } from '@/stores/liveRuns'
import { useWorkspaceTabsStore } from '@/stores/workspaceTabs'
import { EVT_TRASH_CHANGED } from '@/lib/trash'
import { PR_INBOX_WINDOW_OPEN_ROUTE, type PrInboxWindowOpenRoute } from '@/lib/prInboxWindow'
import { CALENDAR_WINDOW_OPEN_ROUTE, type CalendarWindowOpenRoute } from '@/lib/calendarWindow'
import { SCREEN_OPEN_ROUTE, type ScreenOpenRoute } from '@/lib/detachedScreens'
import {
  SLACK_API_RECEIVED,
  type SlackApiReceived,
  SLACK_NOTIFICATION_CLICKED,
  type SlackNotificationClicked,
  openSlackWindow,
  SLACK_WINDOW_OPEN_ROUTE,
  type SlackWindowOpenRoute,
} from '@/lib/slackWindow'
import {
  WINDOW_NEW_SESSION,
  WINDOW_OPEN_RUN,
  createdFromSource,
  linkTargetRun,
  type WindowNewSession,
  type WindowOpenRun,
} from '@/lib/sessionLinks'
import { useChatDraftsStore } from '@/stores/chatDrafts'
import { useToast } from '@/composables/useToast'
import { useRunsStore } from '@/stores/runs'
import { useConductorStore } from '@/stores/conductor'
import { TRAY_OPEN_RUN, type TrayOpenRun } from '@/lib/tray'
import { focusSessionWindow } from '@/lib/sessionWindow'
import { EVT_TRAY_OPEN } from '@/lib/backgroundProcesses'
import { FILE_WINDOW_DOCK, type FileWindowDock } from '@/lib/fileWindow'
import { useSessionTabsStore } from '@/stores/sessionTabs'

/**
 * Call in setup; then await the returned `listenMainWindowEvents()` once, from
 * the main window's `onMounted` (after the menu and the tray are set up).
 * Everything it attached is torn down on unmount.
 */
export function useMainWindowEvents() {
  const router = useRouter()
  const projectsStore = useProjectsStore()
  const tabsStore = useWorkspaceTabsStore()
  const live = useLiveRunsStore()
  const runsStore = useRunsStore()
  const conductorStore = useConductorStore()
  const draftsStore = useChatDraftsStore()
  // Created up front: it adds the docked file's tab itself (stores/sessionTabs).
  useSessionTabsStore()
  const { toast } = useToast()
  const { t } = useI18n()

  // Global listener that mirrors backend-reported active runs into the liveRuns
  // store (see listenMainWindowEvents). Torn down on unmount to avoid a dangling subscription.
  let unlistenActivated: UnlistenFn | null = null

  // Listener for a session row clicked in the menu-bar switcher (see lib/tray.ts).
  let unlistenTrayRun: UnlistenFn | null = null
  let unlistenFocusOpen: UnlistenFn | null = null

  // Listener for the tray's "Background processes" row (lib/tray.ts).
  let unlistenTrayProcs: UnlistenFn | null = null

  // Listener for a clicked Slack thread OS notification (see lib/slackWindow.ts).
  let unlistenCaptureNotification: UnlistenFn | null = null

  // Listener for a Slack thread received over the Inbox API (see lib/slackWindow.ts).
  let unlistenSlackApiReceived: UnlistenFn | null = null

  // Listener for the Slack threads window asking to navigate (e.g. to the MCP settings).
  let unlistenSlackRoute: UnlistenFn | null = null

  // Listener for the PR Inbox window asking to navigate (e.g. "Add to puppeteer").
  let unlistenPrInboxRoute: UnlistenFn | null = null
  let unlistenCalendarRoute: UnlistenFn | null = null
  // Listener for a detachable screen's own window asking for its screen here ("Move back to main window").
  let unlistenScreenRoute: UnlistenFn | null = null

  // Listeners for the Slack / Notes / Todos windows opening a linked session or
  // starting a new one (lib/sessionLinks).
  let unlistenWindowRun: UnlistenFn | null = null
  let unlistenWindowNewSession: UnlistenFn | null = null
  // A file window's "Back to main window" (V3-24).
  let unlistenFileDock: UnlistenFn | null = null
  // Projects moved to / out of the Trash (any window, a remote desktop): the
  // project list (Projects grid, tabs, palette) follows.
  let unlistenTrashChanged: UnlistenFn | null = null

  // A session row in the menu-bar switcher (or a linked session in the Slack /
  // Notes / Todos windows) was clicked: show the run where it lives — its own
  // pop-out if it has one, otherwise this window. `projectHint` covers runs this
  // window never loaded.
  async function openRunFromTray(runId: string, projectHint?: string, preferMain = false) {
    if (!preferMain && (await focusSessionWindow(runId))) return
    const projectId =
      live.sessions.get(runId)?.projectId ?? runsStore.runMeta.get(runId)?.project_id ?? projectHint
    if (!projectId) return
    try {
      const win = getCurrentWebviewWindow()
      await win.unminimize()
      await win.show()
      await win.setFocus()
    } catch {
      /* best-effort focus */
    }
    live.markSeen(runId)
    // Open the project's workspace tab before routing — the run workspace is
    // driven by the tabs store (see PermissionNotifier.navigateToRun).
    tabsStore.open(projectId, runId)
    router
      .push({ name: 'project-run-detail', params: { projectId, runId } })
      .catch(() => {})
  }

  // A router-less pop-out (Slack threads / PR Inbox window) asked to show an app
  // screen: bring this window forward and navigate. Only in-app paths.
  async function openRouteFromWindow(path: string | undefined) {
    if (!path || !path.startsWith('/')) return
    try {
      const win = getCurrentWebviewWindow()
      await win.unminimize()
      await win.show()
      await win.setFocus()
    } catch {
      /* best-effort focus */
    }
    router.push(path).catch(() => {})
  }

  // "New session from this thread / note / todo" in a pop-out: the session is
  // created here, where the drafts store lives, its composer is prefilled with
  // the target's reference (not sent), the target is linked to it, then it opens.
  // One request per target at a time: a double click must not create two sessions.
  const newSessionPending = new Set<string>()
  async function newSessionFromWindow({ projectId, text, target }: WindowNewSession) {
    const key = `${target.kind}:${target.id}`
    if (newSessionPending.has(key)) return
    newSessionPending.add(key)
    try {
      let runId: string
      try {
        runId = (await runsStore.createSessionRun(projectId)).id
      } catch (e) {
        toast.error(t('run.createSessionFailed', { error: String(e) }))
        return
      }
      draftsStore.set(runId, { text, images: [], files: [] })
      try {
        await linkTargetRun(target, runId, createdFromSource(target.kind))
      } catch (e) {
        // Best-effort: the session and its prefilled composer still open.
        console.warn('[links] linking the new session failed', e)
      }
      await runsStore.refreshMeta(projectId).catch(() => {})
      await openRunFromTray(runId, projectId)
    } finally {
      newSessionPending.delete(key)
    }
  }

  onBeforeUnmount(() => {
    unlistenActivated?.()
    unlistenActivated = null
    unlistenTrayRun?.()
    unlistenTrayRun = null
    unlistenFocusOpen?.()
    unlistenFocusOpen = null
    unlistenTrayProcs?.()
    unlistenTrayProcs = null
    unlistenCaptureNotification?.()
    unlistenCaptureNotification = null
    unlistenSlackApiReceived?.()
    unlistenSlackApiReceived = null
    unlistenSlackRoute?.()
    unlistenSlackRoute = null
    unlistenPrInboxRoute?.()
    unlistenPrInboxRoute = null
    unlistenCalendarRoute?.()
    unlistenCalendarRoute = null
    unlistenScreenRoute?.()
    unlistenScreenRoute = null
    unlistenWindowRun?.()
    unlistenWindowRun = null
    unlistenWindowNewSession?.()
    unlistenWindowNewSession = null
    unlistenFileDock?.()
    unlistenFileDock = null
    unlistenTrashChanged?.()
    unlistenTrashChanged = null
  })

  async function listenMainWindowEvents() {
    // Attach per-run listeners for any run the backend reports as active — even
    // ones this window never opened (started/resumed from a remote Controller, or
    // still live after a restart). This is what lets a remotely-driven run's
    // permission / question prompts surface on the desktop (active-runs dock +
    // native notification), not just on the phone. `startListening` is idempotent,
    // so re-announcing a locally-started run is a harmless no-op.
    try {
      unlistenActivated = await listen<{ run_id: string; project_id: string }>(
        'run:activated',
        async (e) => {
          const { run_id, project_id } = e.payload ?? {}
          if (!run_id || !project_id) return
          // `activate` (not just `startListening`): on a backend-driven resume the
          // listeners survived the previous turn, so only this resets the stale
          // terminal status that would otherwise let the session be evicted.
          live.activate(run_id, project_id).catch(() => {})
          // A freshly-spawned run (notably a conductor's worker) lands in
          // live.sessions the instant it activates, but the dock groups workers
          // under their conductor via runMeta[run].conductor_run_id — which only
          // list_runs fills. Without this, a new worker flashes as a standalone
          // row until something opens a RunView for its project and refetches.
          // refreshMeta learns the worker's conductor so the dock nests it at
          // once; when the run is in the on-screen project it also refreshes the
          // foreground runs list (quietly, no loading flash) so RunView's History
          // gives the conductor its expand chevron immediately too.
          if (!runsStore.runMeta.has(run_id)) {
            await runsStore.refreshMeta(project_id).catch(() => {})
          }
          // Worker of the conductor currently open in a RunView? Refresh that
          // conductor's right-side worker panel too. The panel is otherwise driven
          // ONLY by the per-select `conductor:changed` listener — a single point of
          // failure that, if it misses or is torn down, leaves a freshly-spawned
          // worker invisible until the user reopens the session. Piggybacking on
          // this always-on `run:activated` signal (the same one the dock trusts)
          // makes the new worker appear in the panel the instant it activates.
          const meta = runsStore.runMeta.get(run_id)
          if (meta?.conductor_run_id && meta.conductor_run_id === conductorStore.selectedId) {
            conductorStore.refreshDetail().catch(() => {})
          }
        },
      )
    } catch {
      // Ignore (e.g. running outside the Tauri shell during dev in a browser).
    }

    try {
      unlistenTrayRun = await listen<TrayOpenRun>(TRAY_OPEN_RUN, (e) => {
        if (e.payload?.runId) void openRunFromTray(e.payload.runId)
      })
    } catch {
      // Ignore (e.g. running outside the Tauri shell during dev in a browser).
    }

    // A Focus reminder click (or the tray's Focus row) while the Focus window is
    // closed: open it here; it picks the todo up itself (`focus_take_open_todo`).
    try {
      unlistenFocusOpen = await listen<FocusOpenTodo>(FOCUS_OPEN_WINDOW, () => {
        void openFocusWindow()
      })
    } catch {
      // Ignore (e.g. running outside the Tauri shell during dev in a browser).
    }

    // The tray's "Background processes" row: show the Processes screen here.
    try {
      unlistenTrayProcs = await listen(EVT_TRAY_OPEN, () => {
        void openRouteFromWindow('/processes')
      })
    } catch {
      // Ignore (e.g. running outside the Tauri shell during dev in a browser).
    }

    // A clicked "Slack thread translated / failed" notification opens the Slack
    // threads window on that thread. Registered here, past the pop-out / session
    // early returns, so only the main window handles it.
    try {
      unlistenCaptureNotification = await listen<SlackNotificationClicked>(
        SLACK_NOTIFICATION_CLICKED,
        (e) => {
          const { id, kind } = e.payload ?? {}
          if (id && kind === 'slack') void openSlackWindow({ id })
        },
      )
    } catch {
      // Ignore (e.g. running outside the Tauri shell during dev in a browser).
    }

    // A Slack thread sent from the browser extension opens the Slack threads
    // window on it right away (it fills in as the background job progresses).
    try {
      unlistenSlackApiReceived = await listen<SlackApiReceived>(SLACK_API_RECEIVED, (e) => {
        if (e.payload?.id) void openSlackWindow({ id: e.payload.id })
      })
    } catch {
      // Ignore (e.g. running outside the Tauri shell during dev in a browser).
    }

    // The Slack threads window has no router of this app: it asks the main window
    // to focus itself and navigate (e.g. to the MCP settings when no token is set).
    try {
      unlistenSlackRoute = await listen<SlackWindowOpenRoute>(SLACK_WINDOW_OPEN_ROUTE, (e) => {
        void openRouteFromWindow(e.payload?.path)
      })
    } catch {
      // Ignore (e.g. running outside the Tauri shell during dev in a browser).
    }

    // Same for the PR Inbox window (e.g. "Add to puppeteer" → the Projects screen).
    try {
      unlistenPrInboxRoute = await listen<PrInboxWindowOpenRoute>(PR_INBOX_WINDOW_OPEN_ROUTE, (e) => {
        void openRouteFromWindow(e.payload?.path)
      })
    } catch {
      // Ignore (e.g. running outside the Tauri shell during dev in a browser).
    }

    // Same for the Calendar window (e.g. "Reconnect" → the Settings screen).
    try {
      unlistenCalendarRoute = await listen<CalendarWindowOpenRoute>(CALENDAR_WINDOW_OPEN_ROUTE, (e) => {
        void openRouteFromWindow(e.payload?.path)
      })
    } catch {
      // Ignore (e.g. running outside the Tauri shell during dev in a browser).
    }

    // Any detachable screen's own window (lib/detachedScreens), e.g. "Move back to main window".
    try {
      unlistenScreenRoute = await listen<ScreenOpenRoute>(SCREEN_OPEN_ROUTE, (e) => {
        void openRouteFromWindow(e.payload?.path)
      })
    } catch {
      // Ignore (e.g. running outside the Tauri shell during dev in a browser).
    }

    // Linked sessions / "New session from this …" of the Slack, Notes and Todos
    // windows (no router of this app there).
    try {
      unlistenWindowRun = await listen<WindowOpenRun>(WINDOW_OPEN_RUN, (e) => {
        const { projectId, runId, preferMain } = e.payload ?? {}
        if (projectId && runId) void openRunFromTray(runId, projectId, preferMain)
      })
      unlistenWindowNewSession = await listen<WindowNewSession>(WINDOW_NEW_SESSION, (e) => {
        const p = e.payload
        if (p?.projectId && p.target?.id) void newSessionFromWindow(p)
      })
      // Show the file's Session; its tab is added by the sessionTabs store.
      unlistenFileDock = await listen<FileWindowDock>(FILE_WINDOW_DOCK, (e) => {
        const { projectId, runId } = e.payload ?? {}
        if (projectId && runId) void openRunFromTray(runId, projectId)
      })
    } catch {
      // Ignore (e.g. running outside the Tauri shell during dev in a browser).
    }

    try {
      unlistenTrashChanged = await listen(EVT_TRASH_CHANGED, () => void projectsStore.fetchProjects())
    } catch {
      // Ignore (e.g. running outside the Tauri shell during dev in a browser).
    }
  }

  return { listenMainWindowEvents }
}
