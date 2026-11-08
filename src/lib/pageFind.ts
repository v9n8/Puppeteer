// In-page find (⌘F): the pure part of composables/usePageFind.ts — where a query
// matches in a list of text chunks (the DOM text nodes of the searched area),
// stepping between matches, the shortcut, and which area it goes to. Kept free
// of Vue / DOM imports so it can be unit-tested with `node --test`.
import { pickSurface } from './selectionSurface.ts'

/** Prop that switches ⌘F on and off everywhere. */
export const FIND_IN_PAGE_FEATURE = 'text_tools.find_in_page'

/** Stop collecting past this many matches: painting more stalls the page. */
export const MAX_MATCHES = 5000

/** A match: `texts[chunk].slice(start, end)`. */
export interface FindMatch {
  chunk: number
  start: number
  end: number
}

export interface FindOptions {
  caseSensitive?: boolean
  limit?: number
}

/**
 * Non-overlapping matches of `query` in each chunk, in order, at most `limit`.
 * A match never spans two chunks. An empty query matches nothing.
 */
export function findMatches(
  texts: readonly string[],
  query: string,
  { caseSensitive = false, limit = MAX_MATCHES }: FindOptions = {},
): FindMatch[] {
  const matches: FindMatch[] = []
  if (!query || limit <= 0) return matches
  const needle = caseSensitive ? query : query.toLowerCase()
  for (let chunk = 0; chunk < texts.length; chunk++) {
    const hay = caseSensitive ? texts[chunk] : texts[chunk].toLowerCase()
    let idx = hay.indexOf(needle)
    while (idx !== -1) {
      matches.push({ chunk, start: idx, end: idx + needle.length })
      if (matches.length >= limit) return matches
      idx = hay.indexOf(needle, idx + needle.length)
    }
  }
  return matches
}

/** 1-based match `current` moved by `delta`, wrapping around; 0 when `count` is 0. */
export function stepMatch(current: number, count: number, delta: number): number {
  if (count <= 0) return 0
  return ((((current - 1 + delta) % count) + count) % count) + 1
}

/** ⌘F on macOS, Ctrl+F elsewhere. */
export function isFindShortcut(e: { key: string; metaKey: boolean; ctrlKey: boolean }): boolean {
  return (e.metaKey || e.ctrlKey) && (e.key === 'f' || e.key === 'F')
}

export interface FindTargetContext {
  /** This window never gets ⌘F (lib/selectionSurface `isSelectionExcludedWindow`). */
  excludedWindow: boolean
  /** The topmost open modal / drawer: only an area inside it may take ⌘F. */
  overlay: Node | null
  /** Where the user is, strongest first: the focused element, the last one clicked. */
  hints: readonly (Node | null)[]
}

/**
 * The find area ⌘F opens in. `elOf` gives an area's root while it is on screen,
 * null otherwise. None in an excluded window, or when an open overlay holds no
 * area — the page behind it is not what is being read. Otherwise the innermost
 * area holding the first hint that lands in one, else the last mounted.
 */
export function pickFindArea<A>(
  areas: readonly A[],
  elOf: (area: A) => Node | null,
  ctx: FindTargetContext,
): A | null {
  if (ctx.excludedWindow) return null
  const shown = areas.filter((a) => {
    const el = elOf(a)
    return !!el && (!ctx.overlay || ctx.overlay.contains(el))
  })
  for (const hint of ctx.hints) {
    const area = pickSurface(shown, (a) => [elOf(a)], hint, null)
    if (area) return area
  }
  return shown[shown.length - 1] ?? null
}
