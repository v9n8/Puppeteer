// Width of a list-detail screen's left pane (UR-32): clamping and the
// remembered value. Pure so the limits can be tested without a DOM.

export const LIST_PANE_MIN = 224
export const LIST_PANE_MAX = 480
export const LIST_PANE_DEFAULT = 280
/** Arrow-key step of the drag handle. */
export const LIST_PANE_STEP = 16

export function clampPaneWidth(px: number): number {
  if (!Number.isFinite(px)) return LIST_PANE_DEFAULT
  return Math.min(LIST_PANE_MAX, Math.max(LIST_PANE_MIN, Math.round(px)))
}

/** The stored width, or `fallback` when nothing usable is stored. */
export function parsePaneWidth(raw: string | null, fallback: number): number {
  if (raw === null || raw.trim() === '') return clampPaneWidth(fallback)
  const n = Number(raw)
  return Number.isFinite(n) ? clampPaneWidth(n) : clampPaneWidth(fallback)
}
