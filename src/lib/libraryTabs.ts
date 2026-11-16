// Library kind tabs (V3-49): the Library screen and Project › Library › Files
// show one kind at a time, with a count per tab. Pure helpers, so the rules
// (which tab opens, what a search keeps, when the project column folds) are
// tested without a component.
import { haystack, matchesSearch } from './listGroups.ts'
import type { DiskItem, DiskRow, LibraryItemKind } from './libraryDisk.ts'

/** Window width under which the Library's project column folds and the tabs become a select. */
export const LIBRARY_NARROW_QUERY = '(max-width: 1099px)'

/** Items per kind, every kind of `kinds` present (0 included); never changed by a search. */
export function kindCounts(items: readonly Pick<DiskItem, 'kind'>[], kinds: readonly LibraryItemKind[]): Record<string, number> {
  const counts: Record<string, number> = {}
  for (const kind of kinds) counts[kind] = 0
  for (const it of items) if (it.kind in counts) counts[it.kind]++
  return counts
}

/** The rows of one kind that match `search`, by name (then engine), as `kindGroups` sorts them. */
export function tabRows(items: readonly DiskItem[], kind: LibraryItemKind, search: string): DiskRow[] {
  return items
    .filter((i) => i.kind === kind && matchesSearch(haystack(i.name, i.description), search))
    .sort((a, b) => a.name.localeCompare(b.name) || a.engine.localeCompare(b.engine))
    .map((item) => ({ id: item.path, item }))
}

/**
 * The tab to show: the selected file's kind; else `current` while it still has
 * items; else the first kind that has items; else the first kind (an empty place).
 */
export function pickTab(
  items: readonly Pick<DiskItem, 'kind' | 'path'>[],
  kinds: readonly LibraryItemKind[],
  selectedPath: string | null,
  current: LibraryItemKind | null,
): LibraryItemKind {
  const selected = selectedPath ? items.find((i) => i.path === selectedPath) : undefined
  if (selected && kinds.includes(selected.kind)) return selected.kind
  const counts = kindCounts(items, kinds)
  if (current && counts[current] > 0) return current
  return kinds.find((k) => counts[k] > 0) ?? kinds[0]
}

/**
 * Whether the Library's project column is folded. Under the narrow mark it
 * folds unless the user opened it (`override`, reset when the window crosses
 * the mark); from the mark up the saved choice applies.
 */
export function projectListCollapsed(stored: boolean, narrow: boolean, override: boolean | null): boolean {
  return narrow ? (override ?? true) : stored
}
