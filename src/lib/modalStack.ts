/**
 * Open dialogs, oldest first. Each Modal closes on a window Escape listener, so
 * with a dialog opened from inside another (a confirm over Settings) one Escape
 * would close both: only the newest open one may take it. Dialogs render in
 * <body> (a theme's backdrop-filter on the outer panel would trap and clip a
 * nested one), so the stack also orders them, not the DOM.
 */
const stack: symbol[] = []

/** Adds `id` on top and returns its z-index: each newer dialog sits above the older ones. */
export function pushModal(id: symbol): number {
  removeModal(id)
  stack.push(id)
  return 50 + stack.length
}

export function removeModal(id: symbol): void {
  const i = stack.indexOf(id)
  if (i >= 0) stack.splice(i, 1)
}

export function isTopModal(id: symbol): boolean {
  return stack[stack.length - 1] === id
}
