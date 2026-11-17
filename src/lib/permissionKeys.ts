// Keyboard guard for the permission panel: Enter means "Allow" unless the focus is on
// something that Enter already activates (button, <summary>, link) or edits.
type Focusable = { closest?: (selector: string) => unknown; isContentEditable?: boolean } | null | undefined

const ACTIVATABLE = 'button, summary, a, input, textarea, select, [contenteditable]'

export function isActivationTarget(target: unknown): boolean {
  const el = target as Focusable
  if (!el || typeof el.closest !== 'function') return false
  return el.isContentEditable === true || el.closest(ACTIVATABLE) !== null
}
