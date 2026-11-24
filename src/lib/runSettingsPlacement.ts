// Where the Run options panel sits once it is teleported out of the composer.
// It keeps its old look (centred above the ⚙ button) and is clamped to the
// window, so a button near a narrow column's edge no longer cuts it off.
export const PANEL_GAP = 8
export const EDGE_MARGIN = 8

export function placeRunSettingsPanel(
  anchor: { left: number; right: number; top: number },
  panel: { width: number; height: number },
  viewport: { width: number; height: number },
): { left: number; top: number } {
  const centred = (anchor.left + anchor.right) / 2 - panel.width / 2
  // `max` outside `min`: a panel larger than the window keeps its left / top edge visible.
  const left = Math.max(EDGE_MARGIN, Math.min(centred, viewport.width - panel.width - EDGE_MARGIN))
  const above = anchor.top - PANEL_GAP - panel.height
  const top = Math.max(EDGE_MARGIN, Math.min(above, viewport.height - panel.height - EDGE_MARGIN))
  return { left, top }
}
