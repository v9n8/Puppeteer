// Which composer chips fold into the Run options popover (V3-34). The Effort and
// Response style chips go first when the Session pane is narrow: always under
// 700px, and under 1000px while the toolbar also holds the run actions (Cancel /
// Steer / Queue while a turn runs, Rewind and Send while a message is edited).
// `sessionWidth` is the Session pane's own width, not the window's.
export const COMPACT_CHIPS_BELOW = 700
export const WIDE_CHIPS_FROM = 1000

export function hideOptionalChips(sessionWidth: number, opts: { hasRunActions: boolean }): boolean {
  if (sessionWidth < COMPACT_CHIPS_BELOW) return true
  return opts.hasRunActions && sessionWidth < WIDE_CHIPS_FROM
}
