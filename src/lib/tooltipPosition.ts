// Where a tooltip goes next to its anchor (UX-33, InfoTip): above it when it
// fits, otherwise below, centred on the anchor and always kept inside the
// window. Pure, so `node --test` covers it; InfoTip measures the DOM.

export interface Box {
  left: number
  top: number
  width: number
  height: number
}

export interface Size {
  width: number
  height: number
}

export interface TooltipPlacement {
  left: number
  top: number
  side: 'top' | 'bottom'
}

/** Space between the anchor and the tooltip. */
export const TOOLTIP_GAP = 6
/** Space kept between the tooltip and the window edge. */
export const TOOLTIP_EDGE = 8

const clamp = (v: number, min: number, max: number) => Math.max(min, Math.min(v, max))

export function placeTooltip(anchor: Box, tip: Size, viewport: Size): TooltipPlacement {
  const roomAbove = anchor.top - TOOLTIP_GAP - TOOLTIP_EDGE
  const roomBelow = viewport.height - (anchor.top + anchor.height) - TOOLTIP_GAP - TOOLTIP_EDGE
  // Above by default; below only when above is too short and below has more room.
  const side = tip.height <= roomAbove || roomAbove >= roomBelow ? 'top' : 'bottom'
  const wantTop = side === 'top'
    ? anchor.top - TOOLTIP_GAP - tip.height
    : anchor.top + anchor.height + TOOLTIP_GAP
  const wantLeft = anchor.left + anchor.width / 2 - tip.width / 2
  // A tip wider or taller than the window pins to the top-left edge rather than off it.
  return {
    left: Math.max(TOOLTIP_EDGE, clamp(wantLeft, TOOLTIP_EDGE, viewport.width - tip.width - TOOLTIP_EDGE)),
    top: Math.max(TOOLTIP_EDGE, clamp(wantTop, TOOLTIP_EDGE, viewport.height - tip.height - TOOLTIP_EDGE)),
    side,
  }
}
