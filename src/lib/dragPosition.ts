// Keeping a floating panel inside the window while it is dragged or resized
// (composables/useDragPosition, used by QuoteNotePopover). Pure, so
// `node --test` covers it; the composable measures the DOM.

export interface Point {
  x: number
  y: number
}

export interface Size {
  width: number
  height: number
}

export interface SizeLimits {
  minWidth: number
  maxWidth: number
  minHeight: number
}

/** Space kept between a panel and the window edge. */
export const DRAG_EDGE = 8

/**
 * Top-left of a panel of `size` put at `pos`, moved back inside the window.
 * A panel larger than the window pins to its top-left edge rather than off it.
 */
export function clampPosition(pos: Point, size: Size, viewport: Size, edge = DRAG_EDGE): Point {
  return {
    x: Math.max(edge, Math.min(pos.x, viewport.width - size.width - edge)),
    y: Math.max(edge, Math.min(pos.y, viewport.height - size.height - edge)),
  }
}

/**
 * Size of a panel at `pos` resized to `size`: within `limits`, and not past the
 * window's right / bottom edge. In a window too small for both, the minimum wins.
 */
export function clampSize(size: Size, pos: Point, viewport: Size, limits: SizeLimits, edge = DRAG_EDGE): Size {
  return {
    width: Math.max(limits.minWidth, Math.min(size.width, limits.maxWidth, viewport.width - pos.x - edge)),
    height: Math.max(limits.minHeight, Math.min(size.height, viewport.height - pos.y - edge)),
  }
}
