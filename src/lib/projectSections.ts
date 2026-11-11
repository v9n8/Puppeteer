import {
  Settings, KanbanSquare, SlidersHorizontal, MessageSquareText, Github, Gitlab, Cloud, CloudCog,
  Library, Server, GitMerge, Rocket, ShieldCheck,
} from 'lucide-vue-next'

/** Tabs of the project detail column (UX-50), in display order. */
export const PROJECT_GROUPS = [
  { id: 'overview', labelKey: 'projectDetail.groups.overview' },
  // Per-project defaults for new sessions (UX-19) and the project's own response styles.
  { id: 'defaults', labelKey: 'projectDetail.groups.defaults' },
  { id: 'connections', labelKey: 'projectDetail.groups.connections' },
  { id: 'library', labelKey: 'projectDetail.groups.library' },
  { id: 'servers', labelKey: 'projectDetail.groups.servers' },
] as const

export type ProjectGroupId = (typeof PROJECT_GROUPS)[number]['id']

/**
 * Sections of the project detail column, in nav order within their `group` tab.
 * The ids are the old flat tab ids, so `?section=<id>` (and the old
 * `/projects/<id>/settings?section=<id>` link) opens the same panel as before the
 * grouping (`claude`, the Claude account, now lives in `session`, and `skills`
 * and `rules` in `files`: see LEGACY_SECTIONS). `conflicts` (skill and rule sync conflicts) only shows while
 * the project has some.
 */
export const PROJECT_SECTIONS = [
  { id: 'overview', group: 'overview', icon: Settings },
  { id: 'board', group: 'overview', icon: KanbanSquare },
  { id: 'session', group: 'defaults', icon: SlidersHorizontal },
  { id: 'styles', group: 'defaults', icon: MessageSquareText },
  { id: 'github', group: 'connections', icon: Github },
  { id: 'gitlab', group: 'connections', icon: Gitlab },
  { id: 'aws', group: 'connections', icon: Cloud },
  { id: 'azure', group: 'connections', icon: CloudCog },
  // What the project has on disk; read-only, edited on the Library screen (UX-57).
  { id: 'files', group: 'library', icon: Library },
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

/** Old section ids still accepted by `?section=`, and the section they open now. */
const LEGACY_SECTIONS = new Map<unknown, ProjectSectionId>([
  // The Claude account select moved into the session defaults form (UX-19).
  ['claude', 'session'],
  // The old library's per-project switches gave way to the files on disk (UX-57).
  ['skills', 'files'],
  ['rules', 'files'],
])

/** The section a `?section=` query names, or null when it names none. */
export function sectionFromQuery(value: unknown): ProjectSectionId | null {
  return LEGACY_SECTIONS.get(value) ?? PROJECT_SECTIONS.find((s) => s.id === value)?.id ?? null
}

/** The tab a section sits in. */
export function sectionGroup(section: ProjectSectionId): ProjectGroupId {
  return PROJECT_SECTIONS.find((s) => s.id === section)!.group
}

/**
 * The section a tab shows: the requested one while that tab lists it, else the
 * tab's first. Every tab has a section that is always listed.
 */
export function tabSection(tab: ProjectGroupId, requested: ProjectSectionId | null, hasConflicts: boolean): ProjectSectionId {
  const sections = projectNav(PROJECT_SECTIONS, hasConflicts).find((g) => g.id === tab)!.sections
  return (sections.find((s) => s.id === requested) ?? sections[0]).id
}

/**
 * The Projects page query that opens a project at a section (UX-50): what the
 * old `/projects/<id>/settings?section=` page opened. No or an unknown section
 * opens the project on its first tab.
 */
export function projectPageQuery(projectId: string, section?: unknown): Record<string, string> {
  const target = sectionFromQuery(section)
  return target ? { id: projectId, tab: sectionGroup(target), section: target } : { id: projectId }
}
