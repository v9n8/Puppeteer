import { hasActiveFilters, type PrFilters } from './prInboxFilters.ts'
import { isFindShortcut } from './pageFind.ts'

/**
 * The PR inbox search row keeps what the user is typing (`draft`) apart from
 * what the list is filtered by (`applied`): typing alone never re-filters.
 */
export interface PrSearchState {
  draft: string
  applied: string
}

export function searchState(applied = ''): PrSearchState {
  return { draft: applied, applied }
}

/** "Search" button / Enter: the draft becomes the filter. An empty draft lifts the text filter. */
export function applySearch(state: PrSearchState): PrSearchState {
  return { draft: state.draft, applied: state.draft.trim() }
}

/** Esc / x: wipe the text and lift the text filter in one step. */
export function clearSearch(): PrSearchState {
  return searchState('')
}

/** "Clear filters" shows only when there is something applied or being typed to clear. */
export function canClear(state: PrSearchState, filters: PrFilters): boolean {
  return state.applied.trim() !== '' || state.draft.trim() !== '' || hasActiveFilters(filters)
}

/** ⌘F / Ctrl+F focuses the search box. ⌘⇧F is "Search sessions", so Shift is excluded. */
export function isPrSearchShortcut(
  e: { key: string; metaKey: boolean; ctrlKey: boolean; shiftKey?: boolean },
  isMac: boolean,
): boolean {
  return isFindShortcut(e, isMac)
}
