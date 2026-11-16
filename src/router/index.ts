import { createRouter, createWebHistory, type RouteLocationNormalized, type RouteRecordRaw } from 'vue-router'
import { emit } from '@tauri-apps/api/event'
import LibraryView from '../views/LibraryView.vue'
import ServersView from '../views/ServersView.vue'
import ProjectsView from '../views/ProjectsView.vue'
import PrInboxView from '../views/PrInboxView.vue'
import { prInboxTarget } from '../lib/prInboxWindow'
import { calendarTarget } from '../lib/calendarWindow'
import { notesTarget, openNotesWindow } from '../lib/itemListWindow'
import { slackTarget, openSlackThreadsWindow } from '../lib/slackWindow'
import { NOTES_PATH, SLACK_PATH, parseItemQuery, parseSlackQuery } from '../lib/screenRoutes'
import { fileTransferTarget } from '../lib/fileTransferWindow'
import { isRemoteMode } from '../lib/ipc/remote'
import { ganttTarget, openGanttWindow } from '../lib/ganttWindow'
import { openFocusWindow, tasksTarget } from '../lib/focusWindow'
import { focusRedirect, parseTasksQuery } from '../lib/tasksRoute'
import { projectPageQuery } from '../lib/projectSections'
import IssuesGanttView from '../views/IssuesGanttView.vue'
import NotesView from '../views/NotesView.vue'
import SlackView from '../views/SlackView.vue'
import RunView from '../views/RunView.vue'
import SettingsView from '../views/SettingsView.vue'
import StatsView from '../views/StatsView.vue'
import CalendarView from '../views/CalendarView.vue'
import FileTransferView from '../views/FileTransferView.vue'
import TasksView from '../views/TasksView.vue'
import SecretNotesView from '../views/SecretNotesView.vue'
import RunningView from '../views/RunningView.vue'
import BoardView from '../views/BoardView.vue'
import SessionSearchView from '../views/SessionSearchView.vue'
import SchedulesView from '../views/SchedulesView.vue'
import TrashView from '../views/TrashView.vue'
import { NAV_ROUTES } from '../lib/navigation'
import { FALLBACK_PATH, routeFeature } from '../lib/featureGate'
import { useFeaturesStore } from '../stores/features'
import { useWorkspaceTabsStore } from '../stores/workspaceTabs'
import { homePath, sessionNavTarget, sessionPath } from '../lib/sidebarNav'
import { IS_SESSION_WINDOW } from '../lib/windowMode'
import { getOpenMode } from '../lib/windowPlacement'
import { SCREEN_OPEN_ROUTE, screenGuard, type ScreenOpenRoute } from '../lib/detachedScreens'
import { openSettingsWindow } from '../lib/settingsWindow'
import { SECRET_NOTES_PATH, secretNotesTarget } from '../lib/secretNotesWindow'
import {
  isSettingsSectionId, mcpLegacyLocation, resolveSettingsPlace, settingsLocation, settingsPlaceFromRoute, settingsRedirect,
} from '../lib/settingsSections'

/**
 * Settings opens where the user keeps it (lib/windowPlacement): here, or its own
 * window, which comes forward on the asked section while this window stays put.
 * A session pop-out never turns into Settings: it hands the link to the main
 * window or the Settings window. `/settings` alone names no section, so an open
 * Settings window keeps its own.
 */
async function settingsGuard(to: RouteLocationNormalized, from: RouteLocationNormalized) {
  const place = to.name === 'settings'
    ? resolveSettingsPlace(to.query.section)
    : settingsPlaceFromRoute(to.params, to.query)
  const openWindow = () => openSettingsWindow(place)
  if (IS_SESSION_WINDOW) {
    const asked = getOpenMode('settings') === 'window'
      ? openWindow()
      : emit<ScreenOpenRoute>(SCREEN_OPEN_ROUTE, { path: to.fullPath })
    asked.catch((e) => console.error('[router] cannot open Settings from the session window', e))
    return false
  }
  const verdict = await screenGuard({ screen: 'settings', path: '/settings', openWindow })(to, from)
  if (verdict !== true) return verdict
  if (to.name === 'settings') return settingsRedirect(to.query)
  // An old id (`/settings/github`) or an unknown one in the path: its canonical route.
  if (!isSettingsSectionId(to.params.section)) return settingsLocation(place!)
  return true
}

/**
 * Secret notes is a screen of the main window or of its own window (the saved
 * choice, its own window by default). Only the main window may host it: the
 * vault commands refuse every other local webview. Off with the vault's prop.
 */
async function secretNotesGuard(to: RouteLocationNormalized, from: RouteLocationNormalized) {
  if (IS_SESSION_WINDOW) return false
  if (!(await useFeaturesStore().isOnAfterLoad('security.secret_vault'))) return FALLBACK_PATH
  return screenGuard(secretNotesTarget)(to, from)
}

// Servers edit in their list's detail pane now; the old
// editor URLs redirect there so stale links keep working.
function editorRedirects(base: string): RouteRecordRaw[] {
  return [
    { path: `${base}/new`, redirect: { path: base, query: { new: '1' } } },
    { path: `${base}/:id/edit`, redirect: (to) => ({ path: base, query: { id: String(to.params.id) } }) },
  ]
}

const router = createRouter({
  history: createWebHistory(),
  routes: [
    // Start-up lands on the tab used last (the store reads localStorage when created).
    { path: '/', redirect: () => {
      const tabs = useWorkspaceTabsStore()
      return homePath(tabs.tabs, tabs.recent)
    } },
    // Virtual (sidebar Session, Go menu, ⌘K): never rendered or kept in history.
    // The Session of the tab in use, or the Projects screen asking to add one.
    { path: '/session', redirect: () => {
      const tabs = useWorkspaceTabsStore()
      const params = router.currentRoute.value.params.projectId
      const target = sessionNavTarget(tabs.tabs, tabs.recent, typeof params === 'string' ? params : null)
      return target ? sessionPath(target) : { path: '/projects', query: { add: '1' } }
    } },
    { path: '/library', name: 'library', component: LibraryView },
    // The Library replaced these screens (UX-54); old links, editor URLs included, land on it.
    ...['/skills', '/rules', '/agents', '/commands', '/import'].map((base) => ({
      path: `${base}/:rest(.*)*`,
      redirect: '/library',
    })),
    // MCP became Settings › Connections › MCP (V3-51); old links, editor URLs included, land there.
    { path: '/mcp', redirect: (to) => mcpLegacyLocation(to.query) },
    { path: '/mcp/new', redirect: () => mcpLegacyLocation({ new: '1' }) },
    { path: '/mcp/:id/edit', redirect: (to) => mcpLegacyLocation({}, String(to.params.id)) },
    { path: '/servers', name: 'servers', component: ServersView },
    ...editorRedirects('/servers'),
    { path: '/projects', name: 'projects', component: ProjectsView },
    { path: '/search', name: 'session-search', component: SessionSearchView },
    { path: '/schedules', name: 'schedules', component: SchedulesView },
    { path: '/trash', name: 'trash', component: TrashView },
    // PR Inbox, Gantt, Calendar, Notes and Slack are screens of the main window or of
    // their own window (the saved choice, the main window by default; lib/windowPlacement).
    // `?project=` / `?id=` carry over to the window when that is where they live.
    {
      path: '/pr-inbox',
      name: 'pr-inbox',
      component: PrInboxView,
      beforeEnter: screenGuard(prInboxTarget),
    },
    {
      path: '/gantt',
      name: 'gantt',
      component: IssuesGanttView,
      beforeEnter: (to, from) => {
        const project = typeof to.query.project === 'string' ? to.query.project : undefined
        return screenGuard({ ...ganttTarget, openWindow: () => openGanttWindow(project) })(to, from)
      },
    },
    {
      path: NOTES_PATH,
      name: 'notes',
      component: NotesView,
      beforeEnter: (to, from) => {
        const loc = parseItemQuery(to.query)
        return screenGuard({ ...notesTarget, openWindow: () => openNotesWindow(loc) })(to, from)
      },
    },
    {
      path: SLACK_PATH,
      name: 'slack',
      component: SlackView,
      beforeEnter: (to, from) => {
        const { projectId, id, add } = parseSlackQuery(to.query)
        return screenGuard({ ...slackTarget, openWindow: () => openSlackThreadsWindow({ projectId, id: id ?? undefined, add }) })(to, from)
      },
    },
    // Tasks (Focus board + todo list) is a screen of the main window or of its own
    // window (lib/focusWindow, the saved choice). `/focus` and its old links land on it.
    {
      path: '/tasks',
      name: 'tasks',
      component: TasksView,
      beforeEnter: (to, from) => {
        const { view, projectId, id } = parseTasksQuery(to.query)
        return screenGuard({ ...tasksTarget, openWindow: () => openFocusWindow({ view, projectId, id }) })(to, from)
      },
    },
    { path: '/focus', name: 'focus', redirect: (to) => focusRedirect(to.query) },
    { path: '/projects/:projectId', name: 'project-run', component: RunView },
    { path: '/projects/:projectId/run/:runId', name: 'project-run-detail', component: RunView },
    // The project settings page became the tabs of the Projects page (UX-50); old links land there.
    {
      path: '/projects/:projectId/settings',
      name: 'project-settings',
      redirect: (to) => ({ path: '/projects', query: projectPageQuery(String(to.params.projectId), to.query.section) }),
    },
    { path: '/stats', name: 'stats', component: StatsView },
    { path: '/calendar', name: 'calendar', component: CalendarView, beforeEnter: screenGuard(calendarTarget) },
    // Settings is a screen of the main window or of its own window (settingsGuard).
    // `/settings` and old `/settings?section=` links never render: they land on
    // `/settings/<section>`, so ⌘, and the menu keep working.
    { path: '/settings', name: 'settings', component: SettingsView, beforeEnter: settingsGuard },
    { path: '/settings/:section', name: 'settings-section', component: SettingsView, beforeEnter: settingsGuard },
    // File Transfer exists only while controlling another Puppeteer (the remote-desktop
    // window): there it is a screen of that window or of its own, per the saved choice.
    {
      path: '/file-transfer',
      name: 'file-transfer',
      component: FileTransferView,
      beforeEnter: (to, from) => (isRemoteMode() ? screenGuard(fileTransferTarget)(to, from) : '/projects'),
    },
    // Secret notes: a screen here once the user merged it back (secretNotesGuard).
    { path: SECRET_NOTES_PATH, name: 'secret-notes', component: SecretNotesView, beforeEnter: secretNotesGuard },
    { path: '/running', name: 'running', component: RunningView },
    { path: '/board', name: 'board', component: BoardView },
    // The Background processes screen became a tab of Running (V3-53).
    { path: '/processes', redirect: { path: '/running', query: { tab: 'processes' } } },
    // Settings › About replaced the About screen; old links land there.
    { path: '/about', redirect: '/settings/about' },
  ],
})

// A destination whose feature is off is turned away whichever way it was reached
// (typed URL, deep link, a pop-out asking this window to navigate). Runs before
// the `beforeEnter` diverts above, so those never open a window for it.
router.beforeEach(async (to) => {
  const feature = routeFeature(to.path, NAV_ROUTES)
  if (feature === undefined) return true
  return (await useFeaturesStore().isOnAfterLoad(feature)) ? true : FALLBACK_PATH
})

export default router
