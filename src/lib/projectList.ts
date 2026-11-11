// Pure helpers behind the Projects page (UX-49): the list's search, which
// project the detail column shows, and which of its tabs.
import { haystack, matchesSearch } from './listGroups.ts'
import {
  PROJECT_GROUPS, sectionFromQuery, sectionGroup, type ProjectGroupId, type ProjectSectionId,
} from './projectSections.ts'

/** Tabs of the project detail column, in display order: the settings groups (UX-50). */
export const PROJECT_TABS: readonly ProjectGroupId[] = PROJECT_GROUPS.map((g) => g.id)
export type ProjectTab = ProjectGroupId

/** Where the detail column is: a tab, and the section asked for in it (null: the tab's first). */
export interface ProjectPlace {
  tab: ProjectTab
  section: ProjectSectionId | null
}

/** The projects whose name or path holds every term of `query`, in list order. */
export function filterProjects<T extends { name: string; path: string }>(projects: readonly T[], query: string): T[] {
  return projects.filter((p) => matchesSearch(haystack(p.name, p.path), query))
}

/**
 * The project the detail column shows: the one the URL names while it exists
 * (a hidden one too, so hiding the selected project keeps it on screen), else
 * the first listed one, else none.
 */
export function selectedProjectId(
  all: readonly { id: string }[],
  listed: readonly { id: string }[],
  requested: unknown,
): string | null {
  if (typeof requested === 'string' && all.some((p) => p.id === requested)) return requested
  return listed[0]?.id ?? null
}

/** The tab a `?tab=` query names, else the tab of the `?section=` it names, else the first tab. */
export function projectTab(value: unknown, section?: unknown): ProjectTab {
  if ((PROJECT_TABS as readonly unknown[]).includes(value)) return value as ProjectTab
  const target = sectionFromQuery(section)
  return target ? sectionGroup(target) : PROJECT_TABS[0]
}

/** The place a `?tab=` + `?section=` query names. */
export function projectPlace(tab: unknown, section: unknown): ProjectPlace {
  return { tab: projectTab(tab, section), section: sectionFromQuery(section) }
}
