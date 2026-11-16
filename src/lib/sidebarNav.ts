// The sidebar's three groups plus its footer, and the two "where does Session
// go" rules (the Session entry, the `/` redirect). Pure: no Vue, Tauri or `@/`
// imports, so `node --test` can load it. Entries come from lib/navigation
// (shared with the Go menu and ⌘K) plus a few extras the sidebar owns.
import { allowsFeature, type FeatureTree } from './featureGate.ts'

export type SidebarGroupId = 'work' | 'mine' | 'agent' | 'foot'

/** i18n key of each group's heading; the footer has none. */
export const GROUP_LABEL_KEYS: Record<SidebarGroupId, string | null> = {
  work: 'nav.groups.work',
  mine: 'nav.groups.mine',
  agent: 'nav.groups.agent',
  foot: null,
}

/** Sidebar order. An id missing here sorts last, in the order it came in. */
const ORDER: readonly string[] = [
  '/session', '/projects', '/processes', '/schedules',
  '/focus', 'todos', 'notes', '/calendar', '/gantt', '/pr-inbox', 'slack', 'secret-notes',
  '/library', '/mcp', '/servers',
  '/stats', '/trash', '/settings',
]

export interface SidebarSource {
  /** Route path, or the id of an entry only the sidebar has. */
  id: string
  group: SidebarGroupId
  feature?: string
  /** False keeps a route out of the sidebar (still in the Go menu and ⌘K). */
  sidebar?: false
}

export interface SidebarGroup<T extends SidebarSource> {
  id: SidebarGroupId
  labelKey: string | null
  items: T[]
}

const GROUP_IDS: readonly SidebarGroupId[] = ['work', 'mine', 'agent', 'foot']

/**
 * The groups to draw: gated entries dropped, ordered, and a group left empty
 * is left out with its heading. `items` mixes the routes (id = path) with the
 * entries only the sidebar has (window buttons, Trash).
 */
export function sidebarGroups<T extends SidebarSource>(items: readonly T[], tree: FeatureTree): SidebarGroup<T>[] {
  const rank = (id: string) => {
    const i = ORDER.indexOf(id)
    return i === -1 ? ORDER.length : i
  }
  const shown = items.filter((it) => it.sidebar !== false && allowsFeature(tree, it.feature))
  return GROUP_IDS.map((id) => ({
    id,
    labelKey: GROUP_LABEL_KEYS[id],
    items: shown.filter((it) => it.group === id).sort((a, b) => rank(a.id) - rank(b.id)),
  })).filter((g) => g.items.length > 0)
}

/** Whether the entry for `id` is the current one on `routePath`. */
export function navActive(id: string, routePath: string): boolean {
  if (id === '/session') return /^\/projects\/[^/]+(\/run\/[^/]+)?\/?$/.test(routePath)
  if (id === '/projects') return routePath === '/projects' || routePath === '/projects/'
  return routePath === id || routePath.startsWith(`${id}/`)
}

interface TabLike {
  projectId: string
  lastRunId: string | null
}

export interface SessionTarget {
  projectId: string
  runId: string | null
}

/**
 * The Session the Session entry opens: the tab being viewed (`routeProjectId`)
 * wins, else the most recently used project that still has a tab, else the
 * first tab. Null when no project tab is open.
 */
export function sessionNavTarget(
  tabs: readonly TabLike[],
  recent: readonly string[],
  routeProjectId: string | null,
): SessionTarget | null {
  const tab =
    tabs.find((t) => t.projectId === routeProjectId) ??
    recent.map((id) => tabs.find((t) => t.projectId === id)).find((t) => t !== undefined) ??
    tabs[0]
  return tab ? { projectId: tab.projectId, runId: tab.lastRunId } : null
}

/** Where `/` goes at start-up: the tab used last, else the Projects screen. */
export function homePath(tabs: readonly TabLike[], recent: readonly string[]): string {
  const target = sessionNavTarget(tabs, recent, null)
  return target ? `/projects/${encodeURIComponent(target.projectId)}` : '/projects'
}

/** The route for a Session target: the last run when the tab has one. */
export function sessionPath(target: SessionTarget): string {
  const project = encodeURIComponent(target.projectId)
  return target.runId ? `/projects/${project}/run/${encodeURIComponent(target.runId)}` : `/projects/${project}`
}
