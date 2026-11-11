/**
 * Pages of a list shown one page at a time (the session History, UX-62, after
 * AWOG's SessionList): only the current page's rows render, keeping the DOM small.
 */

export const SESSIONS_PAGE_SIZE = 20

export interface ListPage<T> {
  items: T[]
  /** 1-based, clamped to the pages there are. */
  page: number
  /** At least 1, so an empty list still reads "1/1". */
  totalPages: number
}

export function totalPages(count: number, size = SESSIONS_PAGE_SIZE): number {
  return Math.max(1, Math.ceil(count / size))
}

/**
 * The rows of 1-based `page`. A page past the end (rows deleted, a search
 * narrowing the list) shows the last one, without a watcher to fix it up.
 */
export function pageSlice<T>(items: readonly T[], page: number, size = SESSIONS_PAGE_SIZE): ListPage<T> {
  const total = totalPages(items.length, size)
  const current = Math.min(Math.max(1, Math.floor(page) || 1), total)
  const start = (current - 1) * size
  return { items: items.slice(start, start + size), page: current, totalPages: total }
}

/** The 1-based page holding the row at `index`, or `null` when it is not listed (-1). */
export function pageOfIndex(index: number, size = SESSIONS_PAGE_SIZE): number | null {
  return index < 0 ? null : Math.floor(index / size) + 1
}
