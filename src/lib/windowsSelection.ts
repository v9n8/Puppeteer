// Pure rules of marking several cards in the "Open windows" overlay
// (components/windows/WindowsOverlay.vue) and of moving them back to the main
// window. No Vue / Tauri / `@/` imports so `node --test` can load it
// (src/lib/__tests__/windowsSelection.test.mjs).
import type { WindowCard } from './windowsOverlay.ts'

type Labelled = Pick<WindowCard, 'label'>

/** `label` marked if it was not, unmarked if it was. */
export function toggleMark(marks: ReadonlySet<string>, label: string): Set<string> {
  const next = new Set(marks)
  if (next.has(label)) next.delete(label)
  else next.add(label)
  return next
}

/**
 * ⇧-click: every card from `anchor` to `label` (either way, display order of `flat`) gets marked,
 * on top of the marks already there. No anchor on screen: only `label`.
 */
export function rangeMark(
  marks: ReadonlySet<string>,
  flat: readonly Labelled[],
  anchor: string | null,
  label: string,
): Set<string> {
  const next = new Set(marks)
  const to = flat.findIndex((c) => c.label === label)
  if (to < 0) return next
  const from = anchor ? flat.findIndex((c) => c.label === anchor) : -1
  if (from < 0) {
    next.add(label)
    return next
  }
  const [lo, hi] = from <= to ? [from, to] : [to, from]
  for (let i = lo; i <= hi; i++) next.add(flat[i].label)
  return next
}

/** ⌘A: the cards on screen, so a filter limits it to what matches. */
export function markAll(flat: readonly Labelled[]): Set<string> {
  return new Set(flat.map((c) => c.label))
}

/** Marks of cards no longer on screen (window closed, filtered out) are dropped. */
export function pruneMarks(marks: ReadonlySet<string>, flat: readonly Labelled[]): Set<string> {
  const shown = new Set(flat.map((c) => c.label))
  return new Set([...marks].filter((l) => shown.has(l)))
}

export interface MergePlan {
  /** In the order given, the window `here` last: it is the one running the overlay. */
  merge: string[]
  /** Windows with no "Move back to main window" of their own. */
  skip: string[]
}

/** Which of `labels` move back to the main window and which are skipped; labels with no card are left out. */
export function mergePlan(
  cards: readonly Pick<WindowCard, 'label' | 'canMerge'>[],
  labels: Iterable<string>,
  here: string | null,
): MergePlan {
  const byLabel = new Map(cards.map((c) => [c.label, c]))
  const merge: string[] = []
  const skip: string[] = []
  let last: string | null = null
  for (const label of new Set(labels)) {
    const card = byLabel.get(label)
    if (!card) continue
    if (!card.canMerge) skip.push(label)
    else if (label === here) last = label
    else merge.push(label)
  }
  if (last) merge.push(last)
  return { merge, skip }
}

/** A rectangle in one coordinate space (the grid's scroll content, for the selection box). */
export interface Box {
  left: number
  top: number
  right: number
  bottom: number
}

/** The selection box from where the press began to where the pointer is, dragged any way. */
export function boxBetween(x0: number, y0: number, x1: number, y1: number): Box {
  return { left: Math.min(x0, x1), top: Math.min(y0, y1), right: Math.max(x0, x1), bottom: Math.max(y0, y1) }
}

/** Cards the box touches, in the order given: touching is enough, a card need not sit inside it (as in Finder). */
export function boxHits(cards: readonly { label: string; rect: Box }[], box: Box): string[] {
  return cards
    .filter(({ rect }) => rect.left <= box.right && rect.right >= box.left && rect.top <= box.bottom && rect.bottom >= box.top)
    .map((c) => c.label)
}

/**
 * Marks while the box is dragged: the cards it touches replace the marks, or are added to the
 * marks from before the drag when ⇧ / ⌘ was held at the press. A card the box leaves loses its
 * mark unless it had one before.
 */
export function boxMarks(before: ReadonlySet<string>, hits: Iterable<string>, additive: boolean): Set<string> {
  const next = new Set(additive ? before : [])
  for (const label of hits) next.add(label)
  return next
}

/**
 * Pixels to scroll per frame while the pointer is within `zone` of an edge of the scroll area
 * (top = just under the sticky drop strip): faster the closer it gets, up to `max`; negative
 * scrolls up, 0 away from the edges.
 */
export function edgeScrollStep(y: number, top: number, bottom: number, zone: number, max: number): number {
  if (y < top + zone) return -Math.ceil(max * Math.min(1, (top + zone - y) / zone))
  if (y > bottom - zone) return Math.ceil(max * Math.min(1, (y - (bottom - zone)) / zone))
  return 0
}
