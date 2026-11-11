// Library screen (UX-54): the skills, rules, commands and agents the CLIs read,
// straight from the disk (commands/library_disk.rs). The file is the item; a
// scan returns each one's `path`, which every other command takes back.
// A project's tab adds its Memory (CLAUDE.md / AGENTS.md, UX-55), read and
// written through memory_* (commands/memory.rs): that file may not exist yet.
// Pure helpers of LibraryView.vue and components/library/.
import { haystack, matchesSearch, type RowGroup } from './listGroups.ts'
import type { MemoryFile, ProjectMemoryKind } from './memoryFiles.ts'

export const CMD_LIBRARY_SCAN = 'library_scan'
export const CMD_LIBRARY_READ = 'library_read'
/** Local-only, like the delete and duplicate. */
export const CMD_LIBRARY_WRITE = 'library_write'
export const CMD_LIBRARY_DELETE = 'library_delete'
export const CMD_LIBRARY_DUPLICATE = 'library_duplicate_to_global'

export type LibraryScope = 'global' | 'project'
export type DiskItemKind = 'skill' | 'rule' | 'command' | 'agent'
/** `memory`: a project's CLAUDE.md / AGENTS.md, made here by `memoryItem` (not from `library_scan`). */
export type LibraryItemKind = DiskItemKind | 'memory'
/** Display order of the list's groups. */
export const DISK_ITEM_KINDS: readonly DiskItemKind[] = ['skill', 'rule', 'command', 'agent']
/** A project's tab: the four, then its Memory. */
export const PROJECT_ITEM_KINDS: readonly LibraryItemKind[] = [...DISK_ITEM_KINDS, 'memory']

/** `git_tracked`: an edit ends up in a commit. `puppeteer_link`: an edit changes every project using it. */
export type DiskItemStatus = 'file' | 'git_tracked' | 'puppeteer_link' | 'symlink'

/** What `library_scan` returns per item (and `memoryItem` for a memory file). */
export interface DiskItem {
  kind: LibraryItemKind
  engine: 'claude' | 'codex'
  scope: LibraryScope
  name: string
  /** The file to open (a skill's `SKILL.md`): the id every other command takes. */
  path: string
  /** What a delete removes: the skill's folder, or the file. */
  entry: string
  description: string
  mtime: number | null
  hash: string | null
  status: DiskItemStatus
  link_target: string | null
  /** Why it can't be opened (a link out of the project, not UTF-8, too large). */
  error: string | null
  /** Memory only: the file isn't there yet; a save creates it. */
  missing?: boolean
}

/** What `library_read` / `library_write` return. */
export interface DiskFile {
  path: string
  link_target: string | null
  content: string
  size: number
  hash: string
}

/** Error prefix of a save refused because the file changed on disk since it was read. */
export const LIBRARY_CONFLICT = 'library_conflict'
/** Error prefix of a duplicate refused because Global already has that name. */
export const LIBRARY_EXISTS = 'library_exists'

export function isLibraryConflict(err: unknown): boolean {
  return String(err).startsWith(LIBRARY_CONFLICT)
}

export function isLibraryExists(err: unknown): boolean {
  return String(err).startsWith(LIBRARY_EXISTS)
}

/** A list row: ListDetailSidebar keys rows by `id`. */
export interface DiskRow {
  id: string
  item: DiskItem
}

/**
 * One group per kind, in `kinds` order, rows by name. Every kind keeps its
 * group (count 0 included) so they are always there; while searching, only
 * the groups with a match are kept.
 */
export function kindGroups(
  items: readonly DiskItem[],
  search: string,
  labelOf: (kind: LibraryItemKind) => string,
  kinds: readonly LibraryItemKind[] = DISK_ITEM_KINDS,
): RowGroup<DiskRow>[] {
  const searching = search.trim() !== ''
  const shown = items.filter((i) => matchesSearch(haystack(i.name, i.description), search))
  return kinds.map((kind) => ({
    key: kind,
    label: labelOf(kind),
    rows: shown
      .filter((i) => i.kind === kind)
      .sort((a, b) => a.name.localeCompare(b.name) || a.engine.localeCompare(b.engine))
      .map((item) => ({ id: item.path, item })),
  })).filter((g) => !searching || g.rows.length > 0)
}

const MEMORY_FILES: Record<ProjectMemoryKind, { engine: DiskItem['engine']; name: string }> = {
  project_claude: { engine: 'claude', name: 'CLAUDE.md' },
  project_agents: { engine: 'codex', name: 'AGENTS.md' },
}

/**
 * The Memory row of a project's file, from what `memory_read` returned (or
 * why it failed). `root` is the project's folder, for the path of a file that
 * couldn't be read.
 */
export function memoryItem(kind: ProjectMemoryKind, root: string, read: MemoryFile | string): DiskItem {
  const { engine, name } = MEMORY_FILES[kind]
  const file = typeof read === 'string' ? null : read
  const path = file?.path ?? `${root.replace(/[\\/]+$/, '')}/${name}`
  return {
    kind: 'memory',
    engine,
    scope: 'project',
    name,
    path,
    entry: path,
    description: '',
    mtime: null,
    hash: file?.hash ?? null,
    status: file?.link_target ? 'symlink' : 'file',
    link_target: file?.link_target ?? null,
    error: typeof read === 'string' ? read : null,
    missing: !!file && !file.exists,
  }
}

/** The memory kind of a Memory row: its engine names the file. */
export function memoryKindOf(item: Pick<DiskItem, 'engine'>): ProjectMemoryKind {
  return item.engine === 'claude' ? 'project_claude' : 'project_agents'
}

/** A delete removes the link, not what it points at; a file or skill folder goes to the trash. */
export function deletesLink(item: Pick<DiskItem, 'status'>): boolean {
  return item.status === 'puppeteer_link' || item.status === 'symlink'
}

/** Where the screen is, as the URL query keeps it (`?scope=project&project=<id>&path=<file>`). */
export interface LibraryPlace {
  scope: LibraryScope
  projectId: string | null
  path: string | null
}

export function libraryPlace(query: Record<string, unknown>): LibraryPlace {
  const str = (v: unknown) => (typeof v === 'string' && v !== '' ? v : null)
  const projectId = str(query.project)
  // A project in the link means its tab, even without `scope`.
  const scope: LibraryScope = query.scope === 'project' || (query.scope !== 'global' && projectId) ? 'project' : 'global'
  return { scope, projectId: scope === 'project' ? projectId : null, path: str(query.path) }
}

export function libraryQuery(place: LibraryPlace): Record<string, string> {
  const q: Record<string, string> = { scope: place.scope }
  if (place.scope === 'project' && place.projectId) q.project = place.projectId
  if (place.path) q.path = place.path
  return q
}
