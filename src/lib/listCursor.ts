// Cursor over a flat list driven by the keyboard (Session search results, chart days).
// Pure so the rules are testable; -1 means "no item selected" (focus stays in the box).

export type CursorKey = 'ArrowUp' | 'ArrowDown' | 'Home' | 'End'

/** The index after `key`. ↓ from -1 enters at 0, ↓ stops at the last item, ↑ from 0 leaves to -1. */
export function moveCursor(index: number, count: number, key: CursorKey): number {
  if (count <= 0) return -1
  switch (key) {
    case 'Home':
      return 0
    case 'End':
      return count - 1
    case 'ArrowDown':
      return Math.min(index + 1, count - 1)
    case 'ArrowUp':
      return index <= 0 ? -1 : Math.min(index - 1, count - 1)
  }
}

export interface FlatHit {
  groupIndex: number
  offset: number
}

/** Every hit of every group in on-screen order. */
export function flatHits(groups: { hits: { offset: number }[] }[]): FlatHit[] {
  const out: FlatHit[] = []
  groups.forEach((g, groupIndex) => {
    for (const h of g.hits) out.push({ groupIndex, offset: h.offset })
  })
  return out
}
