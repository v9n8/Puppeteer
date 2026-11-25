// Library kind tabs (V3-49): the Library screen and Project › Library › Files
// show one kind at a time, with a count per tab. Pure helpers, so the rules
// (which tab opens, what a search keeps, when the project column folds) are
// tested without a component.
import { haystack, matchesSearch } from './listGroups.ts'
import { DISK_ITEM_KINDS, type DiskItem, type DiskRow, type LibraryItemKind, type LibraryScope } from './libraryDisk.ts'
import { CONFIG_ITEM_KINDS } from './libraryConfig.ts'

/** Window width under which the Library's project column folds and the tabs become a select. */
export const LIBRARY_NARROW_QUERY = '(max-width: 1099px)'

/**
 * The Library screen's kinds: the files, then hooks and plugins (left out in
 * remote mode: their scan is local-only), then a project's Memory.
 */
export function libraryKinds(scope: LibraryScope, remote: boolean): LibraryItemKind[] {
  return [...DISK_ITEM_KINDS, ...(remote ? [] : CONFIG_ITEM_KINDS), ...(scope === 'project' ? (['memory'] as const) : [])]
}

/** Items per kind, every kind of `kinds` present (0 included); never changed by a search. */
export function kindCounts(items: readonly { kind: LibraryItemKind }[], kinds: readonly LibraryItemKind[]): Record<string, number> {
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
  items: readonly { kind: LibraryItemKind; path: string; hash?: string | null }[],
  kinds: readonly LibraryItemKind[],
  selectedPath: string | null,
  current: LibraryItemKind | null,
): LibraryItemKind {
  const selected = selectedPath ? items.find((i) => i.path === selectedPath) : undefined
  if (selected && kinds.includes(selected.kind)) return selected.kind
  // A Memory row is listed even when its file is missing (`hash` null): that is not content.
  const counts = kindCounts(items.filter((i) => i.kind !== 'memory' || i.hash !== null), kinds)
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
