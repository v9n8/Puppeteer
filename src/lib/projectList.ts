// Pure helpers behind the Projects page (UX-49): the list's search, which
// project the detail column shows, and which of its tabs.
import { haystack, matchesSearch } from './listGroups.ts'

/** Tabs of the project detail column, in display order. */
export const PROJECT_TABS = ['overview'] as const
export type ProjectTab = (typeof PROJECT_TABS)[number]

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

/** The tab a `?tab=` query names, or the first tab. */
export function projectTab(value: unknown): ProjectTab {
  return (PROJECT_TABS as readonly unknown[]).includes(value) ? (value as ProjectTab) : PROJECT_TABS[0]
}
