import type { Component } from 'vue'
import {
  Server,
  HardDrive,
  FolderOpen,
  GitPullRequest,
  GanttChartSquare,
  BarChart3,
  CalendarDays,
  Target,
  Cpu,
  Settings,
  Search,
  CalendarClock,
  Library,
  MessageSquare,
} from 'lucide-vue-next'
import { openPrInboxWindow } from '@/lib/prInboxWindow'
import { openCalendarWindow } from '@/lib/calendarWindow'
import { openGanttWindow } from '@/lib/ganttWindow'
import { openFocusWindow } from '@/lib/focusWindow'
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
   * Set for destinations that live in their own OS window (like the Slack
   * threads window): the sidebar entry and the Go menu item call this instead
   * of routing, and the route itself diverts to it.
   */
  openWindow?: () => unknown
  /**
   * Module or prop id (lib/featureGate) the destination belongs to: while it is
   * off, the sidebar and Go menu drop the entry and the router turns the path away.
   */
  feature?: string
  /** False keeps it out of the sidebar; the Go menu and ⌘K still list it. */
  sidebar?: false
  /** The screen whose saved open mode (lib/windowPlacement) marks the entry with ↗. */
  screen?: DetachableScreen
}

export const NAV_ROUTES: NavRoute[] = [
  // Virtual: the router sends it to the Session of the tab in use (or asks to add a Project).
  { path: '/session', labelKey: 'nav.session', icon: MessageSquare, group: 'work' },
  { path: '/projects', labelKey: 'nav.projects', icon: FolderOpen, group: 'work' },
  { path: '/processes', labelKey: 'nav.processes', icon: Cpu, group: 'work' },
  { path: '/schedules', labelKey: 'nav.schedules', icon: CalendarClock, group: 'work', feature: 'orchestration.schedules' },
  { path: '/search', labelKey: 'nav.search', icon: Search, group: 'work', sidebar: false, feature: 'sessions.cross_session_search' },
  { path: '/focus', labelKey: 'nav.focus', icon: Target, group: 'mine', openWindow: openFocusWindow, feature: 'productivity.focus' },
  { path: '/calendar', labelKey: 'nav.calendar', icon: CalendarDays, group: 'mine', openWindow: () => openCalendarWindow(), feature: 'productivity.calendar', screen: 'calendar' },
  { path: '/gantt', labelKey: 'nav.gantt', icon: GanttChartSquare, group: 'mine', openWindow: () => openGanttWindow(), feature: 'code_hosting.gantt', screen: 'gantt' },
  { path: '/pr-inbox', labelKey: 'nav.prInbox', icon: GitPullRequest, group: 'mine', openWindow: openPrInboxWindow, feature: 'code_hosting.pr_inbox', screen: 'pr-inbox' },
  { path: '/library', labelKey: 'nav.library', icon: Library, group: 'agent' },
  { path: '/mcp', labelKey: 'nav.mcp', icon: Server, group: 'agent' },
  { path: '/servers', labelKey: 'nav.servers', icon: HardDrive, group: 'agent', feature: 'infra_access.vps' },
  { path: '/stats', labelKey: 'nav.stats', icon: BarChart3, group: 'foot', feature: 'usage.stats' },
  { path: '/settings', labelKey: 'nav.settings', icon: Settings, group: 'foot', screen: 'settings' },
]
