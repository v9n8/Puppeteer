import type { Component } from 'vue'
import {
  Puzzle,
  ScrollText,
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
  Info,
  Search,
  Bot,
  SquareSlash,
} from 'lucide-vue-next'
import { openPrInboxWindow } from '@/lib/prInboxWindow'
import { openCalendarWindow } from '@/lib/calendarWindow'
import { openGanttWindow } from '@/lib/ganttWindow'
import { openFocusWindow } from '@/lib/focusWindow'

/**
 * The app's top-level destinations, in sidebar order. Shared by the sidebar and
 * the native "Go" menu so the two can never drift — adding a screen in one
 * place adds it to both.
 *
 * Ordered by how often they're used: daily work first, then the agent
 * configuration library, then app-level screens. A divider is drawn wherever
 * `group` changes.
 */
export type NavGroup = 'work' | 'config' | 'app'

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
}

export const NAV_ROUTES: NavRoute[] = [
  { path: '/projects', labelKey: 'nav.projects', icon: FolderOpen, group: 'work' },
  { path: '/search', labelKey: 'nav.search', icon: Search, group: 'work', feature: 'sessions.cross_session_search' },
  { path: '/focus', labelKey: 'nav.focus', icon: Target, group: 'work', openWindow: openFocusWindow, feature: 'productivity.focus' },
  { path: '/pr-inbox', labelKey: 'nav.prInbox', icon: GitPullRequest, group: 'work', openWindow: openPrInboxWindow, feature: 'code_hosting.pr_inbox' },
  { path: '/calendar', labelKey: 'nav.calendar', icon: CalendarDays, group: 'work', openWindow: () => openCalendarWindow(), feature: 'productivity.calendar' },
  { path: '/gantt', labelKey: 'nav.gantt', icon: GanttChartSquare, group: 'work', openWindow: () => openGanttWindow(), feature: 'code_hosting.gantt' },
  { path: '/skills', labelKey: 'nav.skills', icon: Puzzle, group: 'config' },
  { path: '/rules', labelKey: 'nav.rules', icon: ScrollText, group: 'config' },
  { path: '/agents', labelKey: 'nav.agents', icon: Bot, group: 'config', feature: 'library.agents' },
  { path: '/commands', labelKey: 'nav.commands', icon: SquareSlash, group: 'config', feature: 'library.commands' },
  { path: '/mcp', labelKey: 'nav.mcp', icon: Server, group: 'config' },
  { path: '/servers', labelKey: 'nav.servers', icon: HardDrive, group: 'config', feature: 'infra_access.vps' },
  { path: '/stats', labelKey: 'nav.stats', icon: BarChart3, group: 'app', feature: 'usage.stats' },
  { path: '/processes', labelKey: 'nav.processes', icon: Cpu, group: 'app' },
  { path: '/settings', labelKey: 'nav.settings', icon: Settings, group: 'app' },
  { path: '/about', labelKey: 'nav.about', icon: Info, group: 'app' },
]
