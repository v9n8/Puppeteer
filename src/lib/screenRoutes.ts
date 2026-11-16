// Addresses of the Notes, Slack and Gantt screens in the main window
// (`/notes?id=`, `/slack?id=&project=&add=1`, `/gantt?project=`). Pure (no Vue,
// Tauri or `@/` imports) so `node --test` can load it. The route guard, the
// "open this note / thread" helpers and Merge back all build and read them here.

export const NOTES_PATH = '/notes'
export const SLACK_PATH = '/slack'
export const GANTT_PATH = '/gantt'

export interface ItemScreenLocation {
  /** A project id, or `__none__` for items without one. */
  projectId?: string | null
  /** The note / thread to select. */
  id?: string | null
}

export interface SlackLocation extends ItemScreenLocation {
  /** Open the "Add thread" dialog. */
  add?: boolean
}

function withQuery(path: string, query: Record<string, string>): string {
  const qs = new URLSearchParams(query).toString()
  return qs ? `${path}?${qs}` : path
}

function itemQuery(loc: ItemScreenLocation): Record<string, string> {
  const q: Record<string, string> = {}
  if (loc.projectId) q.project = loc.projectId
  if (loc.id) q.id = loc.id
  return q
}

export function notesPath(loc: ItemScreenLocation = {}): string {
  return withQuery(NOTES_PATH, itemQuery(loc))
}

export function slackPath(loc: SlackLocation = {}): string {
  const q = itemQuery(loc)
  if (loc.add) q.add = '1'
  return withQuery(SLACK_PATH, q)
}

export function ganttPath(projectId?: string | null): string {
  return withQuery(GANTT_PATH, projectId ? { project: projectId } : {})
}

function first(v: unknown): string | null {
  const s = Array.isArray(v) ? v[0] : v
  return typeof s === 'string' && s !== '' ? s : null
}

/** Reads a router query back (`project`, `id`, `add`); anything missing is null / false. */
export function parseSlackQuery(query: Record<string, unknown>): Required<SlackLocation> {
  return { projectId: first(query.project), id: first(query.id), add: first(query.add) === '1' }
}

export function parseItemQuery(query: Record<string, unknown>): Required<ItemScreenLocation> {
  const { projectId, id } = parseSlackQuery(query)
  return { projectId, id }
}
