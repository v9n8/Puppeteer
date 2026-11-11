/**
 * Height of a text field that grows with its text and that the user can also
 * size by hand (UX-65): the text grows it up to `cap`, then it scrolls; a height
 * the user dragged is a floor it never shrinks below, and may exceed the cap.
 */
export function autoGrowHeight(contentHeight: number, cap: number, manual: number | null = null): number {
  const grown = Math.min(contentHeight, cap)
  return manual != null && manual > grown ? manual : grown
}

/** A dragged height kept within `min` and `max` (rounded to whole pixels). */
export function clampDragHeight(height: number, min: number, max: number): number {
  return Math.round(Math.min(Math.max(height, min), Math.max(min, max)))
}
