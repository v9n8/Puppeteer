import type { Component } from 'vue'
import {
  HardDrive,
  FolderOpen,
  GitPullRequest,
  GanttChartSquare,
  BarChart3,
  CalendarDays,
  Target,
  Activity,
  Kanban,
  Settings,
  Search,
  CalendarClock,
  Library,
  MessageSquare,
  Slack,
  StickyNote,
} from 'lucide-vue-next'
import { TASKS_FEATURE } from '@/lib/featureGate'
import type { DetachableScreen } from '@/lib/windowPlacementCore'
import type { SidebarGroupId } from '@/lib/sidebarNav'

/**
 * The app's top-level destinations. Shared by the sidebar, the native "Go" menu
 * and ⌘K so they can never drift — adding a screen in one place adds it to all.
 *
 * `group` is the sidebar group (lib/sidebarNav owns the order inside it); the Go
 * menu draws a divider wherever it changes, so keep a group's routes together
 * here.
 */
export type NavGroup = SidebarGroupId

export interface NavRoute {
  path: string
  /** i18n key under the `nav` namespace. */
  labelKey: string
  icon: Component
  group: NavGroup
  /**
   * Module or prop id (lib/featureGate) the destination belongs to: while it is
   * off, the sidebar and Go menu drop the entry and the router turns the path away.
   */
  feature?: string
  /** i18n key of the ⌘K row when it reads differently from the sidebar label. */
  paletteLabelKey?: string
  /** False keeps it out of the sidebar; the Go menu and ⌘K still list it. */
  sidebar?: false
  /** The screen whose saved open mode (lib/windowPlacement) marks the entry with ↗. */
  screen?: DetachableScreen
}

export const NAV_ROUTES: NavRoute[] = [
  // Virtual: the router sends it to the Session of the tab in use (or asks to add a Project).
  { path: '/session', labelKey: 'nav.session', icon: MessageSquare, group: 'work' },
  { path: '/board', labelKey: 'nav.board', icon: Kanban, group: 'work', feature: 'sessions.board', paletteLabelKey: 'board.nav' },
  { path: '/projects', labelKey: 'nav.projects', icon: FolderOpen, group: 'work' },
  { path: '/running', labelKey: 'nav.running', icon: Activity, group: 'work' },
  { path: '/schedules', labelKey: 'nav.schedules', icon: CalendarClock, group: 'work', feature: 'orchestration.schedules' },
  { path: '/search', labelKey: 'nav.search', icon: Search, group: 'work', sidebar: false, feature: 'sessions.cross_session_search' },
    // The router's screen guard sends a screen to its own window when that is where the user keeps it.
  { path: '/tasks', labelKey: 'nav.tasks', icon: Target, group: 'mine', feature: TASKS_FEATURE, screen: 'tasks' },
  { path: '/notes', labelKey: 'item.navNotes', icon: StickyNote, group: 'mine', feature: 'productivity.notes', screen: 'notes' },
  { path: '/calendar', labelKey: 'nav.calendar', icon: CalendarDays, group: 'mine', feature: 'productivity.calendar', screen: 'calendar' },
  { path: '/gantt', labelKey: 'nav.gantt', icon: GanttChartSquare, group: 'mine', feature: 'code_hosting.gantt', screen: 'gantt' },
  { path: '/pr-inbox', labelKey: 'nav.prInbox', icon: GitPullRequest, group: 'mine', feature: 'code_hosting.pr_inbox', screen: 'pr-inbox' },
  { path: '/slack', labelKey: 'slackThread.nav', icon: Slack, group: 'mine', feature: 'integrations.slack', screen: 'slack' },
  { path: '/library', labelKey: 'nav.library', icon: Library, group: 'agent' },
  { path: '/servers', labelKey: 'nav.servers', icon: HardDrive, group: 'agent', feature: 'infra_access.vps' },
  { path: '/stats', labelKey: 'nav.stats', icon: BarChart3, group: 'foot', feature: 'usage.stats' },
  { path: '/settings', labelKey: 'nav.settings', icon: Settings, group: 'foot', screen: 'settings' },
]
