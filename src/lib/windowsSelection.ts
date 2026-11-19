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
