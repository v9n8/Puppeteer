// The Tasks screen's address: `/tasks?view=focus|list[&project=][&id=]`. Pure (no
// Vue, Tauri or `@/` imports) so `node --test` can load it. The route, the
// pop-out URL, "open this todo" from elsewhere and the redirect from the old
// `/focus` all go through these helpers.

export const TASKS_PATH = '/tasks'

export type TaskView = 'focus' | 'list'

const VIEWS: readonly TaskView[] = ['focus', 'list']

export interface TasksLocation {
  view?: TaskView | null
  /** A project id, or `__none__` for todos without one. */
  projectId?: string | null
  /** The todo to show: selected in the list, opened in the Focus panel. */
  id?: string | null
}

export function isTaskView(v: unknown): v is TaskView {
  return typeof v === 'string' && (VIEWS as readonly string[]).includes(v)
}

/** The query of a location; keys without a value are left out. */
export function tasksQuery(loc: TasksLocation): Record<string, string> {
  const q: Record<string, string> = {}
  if (loc.view && isTaskView(loc.view)) q.view = loc.view
  if (loc.projectId) q.project = loc.projectId
  if (loc.id) q.id = loc.id
  return q
}

function first(v: unknown): string | null {
  const s = Array.isArray(v) ? v[0] : v
  return typeof s === 'string' && s !== '' ? s : null
}

/** Reads a router query (or the URL's) back; anything unknown becomes null. */
export function parseTasksQuery(query: Record<string, unknown>): Required<TasksLocation> {
  const view = first(query.view)
  return {
    view: isTaskView(view) ? view : null,
    projectId: first(query.project),
    id: first(query.id),
  }
}

/** The views on offer: each follows its own prop (`productivity.focus`, `productivity.todos`). */
export function visibleTaskViews(focusOn: boolean, todosOn: boolean): TaskView[] {
  return [...(focusOn ? (['focus'] as const) : []), ...(todosOn ? (['list'] as const) : [])]
}

/**
 * The view to show: the one asked for, else the one used last, else Focus — each
 * only while it is on offer; with one view left that one wins. Null when none is.
 */
export function resolveTaskView(
  wanted: TaskView | null,
  saved: TaskView | null,
  visible: readonly TaskView[],
): TaskView | null {
  if (wanted && visible.includes(wanted)) return wanted
  if (saved && visible.includes(saved)) return saved
  return visible[0] ?? null
}

/** `/focus` (and its old links) land on the Tasks screen in the Focus view, the query kept. */
export function focusRedirect(query: Record<string, unknown>): { path: string; query: Record<string, string> } {
  const { projectId, id } = parseTasksQuery(query)
  return { path: TASKS_PATH, query: tasksQuery({ view: 'focus', projectId, id }) }
}

/** `path?query` for an event that carries a route (SCREEN_OPEN_ROUTE). */
export function tasksPath(loc: TasksLocation): string {
  const qs = new URLSearchParams(tasksQuery(loc)).toString()
  return qs ? `${TASKS_PATH}?${qs}` : TASKS_PATH
}
