// Adds `handler` on the next macrotask and returns a remover that also cancels
// a pending add. A dialog opened by a key press registers its Escape listener
// while that very event is still bubbling to `window`; without the delay the
// same Esc that opened it would close it again.
export interface KeyTarget {
  addEventListener(type: 'keydown', fn: (e: KeyboardEvent) => void): void
  removeEventListener(type: 'keydown', fn: (e: KeyboardEvent) => void): void
}

export function addKeydownNextTick(target: KeyTarget, handler: (e: KeyboardEvent) => void): () => void {
  const timer = setTimeout(() => target.addEventListener('keydown', handler), 0)
  return () => {
    clearTimeout(timer)
    target.removeEventListener('keydown', handler)
  }
}
