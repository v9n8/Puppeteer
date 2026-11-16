import { createRouter, createWebHistory, type RouteLocationNormalized, type RouteRecordRaw } from 'vue-router'
import { emit } from '@tauri-apps/api/event'
import LibraryView from '../views/LibraryView.vue'
import McpServersView from '../views/McpServersView.vue'
import ServersView from '../views/ServersView.vue'
import ProjectsView from '../views/ProjectsView.vue'
import PrInboxView from '../views/PrInboxView.vue'
import { openPrInboxWindow } from '../lib/prInboxWindow'
import { openCalendarWindow } from '../lib/calendarWindow'
import { fileTransferTarget } from '../lib/fileTransferWindow'
import { isRemoteMode } from '../lib/ipc/remote'
import { openGanttWindow } from '../lib/ganttWindow'
import { openFocusWindow } from '../lib/focusWindow'
import { projectPageQuery } from '../lib/projectSections'
import IssuesGanttView from '../views/IssuesGanttView.vue'
import RunView from '../views/RunView.vue'
import SettingsView from '../views/SettingsView.vue'
import StatsView from '../views/StatsView.vue'
import CalendarView from '../views/CalendarView.vue'
import FileTransferView from '../views/FileTransferView.vue'
import FocusView from '../views/FocusView.vue'
import AboutView from '../views/AboutView.vue'
import BackgroundProcessesView from '../views/BackgroundProcessesView.vue'
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
import {
  isSettingsSectionId, resolveSettingsPlace, settingsLocation, settingsPlaceFromRoute, settingsRedirect,
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

// MCP / Servers edit in their list's detail pane now; the old
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
    { path: '/mcp', name: 'mcp', component: McpServersView },
    ...editorRedirects('/mcp'),
    { path: '/servers', name: 'servers', component: ServersView },
    ...editorRedirects('/servers'),
    { path: '/projects', name: 'projects', component: ProjectsView },
    { path: '/search', name: 'session-search', component: SessionSearchView },
    { path: '/schedules', name: 'schedules', component: SchedulesView },
    { path: '/trash', name: 'trash', component: TrashView },
    // The PR Inbox lives in its own window (lib/prInboxWindow), like the Slack
    // threads window; the route only stays so old links / navigation open it.
    // PrInboxView is never rendered here — the guard always diverts.
    {
      path: '/pr-inbox',
      name: 'pr-inbox',
      component: PrInboxView,
      beforeEnter: (_to, from) => {
        void openPrInboxWindow()
        // Stay where we are; on a cold start (nothing matched yet) land on the default screen.
        return from.matched.length > 0 ? false : '/projects'
      },
    },
    // Gantt and Focus too (lib/ganttWindow, lib/focusWindow). `?project=` deep
    // links (Projects screen) carry over to the Gantt window.
    {
      path: '/gantt',
      name: 'gantt',
      component: IssuesGanttView,
      beforeEnter: (to, from) => {
        void openGanttWindow(typeof to.query.project === 'string' ? to.query.project : undefined)
        return from.matched.length > 0 ? false : '/projects'
      },
    },
    {
      path: '/focus',
      name: 'focus',
      component: FocusView,
      beforeEnter: (_to, from) => {
        void openFocusWindow()
        return from.matched.length > 0 ? false : '/projects'
      },
    },
    { path: '/projects/:projectId', name: 'project-run', component: RunView },
    { path: '/projects/:projectId/run/:runId', name: 'project-run-detail', component: RunView },
    // The project settings page became the tabs of the Projects page (UX-50); old links land there.
    {
      path: '/projects/:projectId/settings',
      name: 'project-settings',
      redirect: (to) => ({ path: '/projects', query: projectPageQuery(String(to.params.projectId), to.query.section) }),
    },
    { path: '/stats', name: 'stats', component: StatsView },
    // Same for the Calendar (lib/calendarWindow): CalendarView is rendered only
    // in that window; the route diverts to it.
    {
      path: '/calendar',
      name: 'calendar',
      component: CalendarView,
      beforeEnter: (_to, from) => {
        void openCalendarWindow()
        return from.matched.length > 0 ? false : '/projects'
      },
    },
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
    { path: '/processes', name: 'processes', component: BackgroundProcessesView },
    { path: '/about', name: 'about', component: AboutView },
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
