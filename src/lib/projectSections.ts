import {
  Settings, KanbanSquare, UserCircle, Github, Gitlab, Cloud, CloudCog,
  Puzzle, ScrollText, Server, GitMerge, Rocket, ShieldCheck,
} from 'lucide-vue-next'

/** Headings of the project settings nav, in display order. */
export const PROJECT_GROUPS = [
  { id: 'overview', labelKey: 'projectDetail.groups.overview' },
  // Per-project defaults for new sessions; UX-19 adds engine, model, permission mode…
  { id: 'defaults', labelKey: 'projectDetail.groups.defaults' },
  { id: 'connections', labelKey: 'projectDetail.groups.connections' },
  { id: 'library', labelKey: 'projectDetail.groups.library' },
  { id: 'servers', labelKey: 'projectDetail.groups.servers' },
] as const

export type ProjectGroupId = (typeof PROJECT_GROUPS)[number]['id']

/**
 * Sections of the project settings screen, in nav order within their `group`.
 * The ids are the old flat tab ids, so `/projects/<id>/settings?section=<id>`
 * opens the same panel as before the grouping. `conflicts` (skill and rule sync
 * conflicts) only shows while the project has some.
 */
export const PROJECT_SECTIONS = [
  { id: 'overview', group: 'overview', icon: Settings },
  { id: 'board', group: 'overview', icon: KanbanSquare },
  { id: 'claude', group: 'defaults', icon: UserCircle },
  { id: 'github', group: 'connections', icon: Github },
  { id: 'gitlab', group: 'connections', icon: Gitlab },
  { id: 'aws', group: 'connections', icon: Cloud },
  { id: 'azure', group: 'connections', icon: CloudCog },
  { id: 'skills', group: 'library', icon: Puzzle },
  { id: 'rules', group: 'library', icon: ScrollText },
  { id: 'mcp', group: 'library', icon: Server },
  { id: 'conflicts', group: 'library', icon: GitMerge },
  { id: 'deploy', group: 'servers', icon: Rocket },
  { id: 'tools', group: 'servers', icon: ShieldCheck },
] as const

export type ProjectSectionId = (typeof PROJECT_SECTIONS)[number]['id']

/**
 * The nav: each group with its visible sections, in group order. A group left
 * with no section is dropped, so no heading is drawn over nothing.
 */
export function projectNav<S extends { id: string; group: ProjectGroupId }>(
  sections: readonly S[],
  hasConflicts: boolean,
): { id: ProjectGroupId; labelKey: string; sections: S[] }[] {
  const visible = sections.filter((s) => s.id !== 'conflicts' || hasConflicts)
  return PROJECT_GROUPS
    .map((g) => ({ id: g.id, labelKey: g.labelKey, sections: visible.filter((s) => s.group === g.id) }))
    .filter((g) => g.sections.length > 0)
}

/** The section a `?section=` query names, or null when it names none. */
export function sectionFromQuery(value: unknown): ProjectSectionId | null {
  return PROJECT_SECTIONS.find((s) => s.id === value)?.id ?? null
}
