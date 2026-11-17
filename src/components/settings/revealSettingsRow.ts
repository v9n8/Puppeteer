// After the Settings search opens a place: waits for the row (a section may mount
// on demand or still be loading), scrolls to it, lights it for two seconds and
// puts the focus on its first control. The row is the visible element carrying
// `data-settings-id="<id>"` (SettingsRow `search-id`, or a Card / button).
const HIT_CLASSES = ['ring-2', 'ring-primary', 'rounded-md', 'bg-primary/10']
const HIT_MS = 2000
const MAX_FRAMES = 40

const FIRST_CONTROL = 'input, select, textarea, [role="switch"], [role="combobox"]'
// A row made only of buttons (Font, Mode, Theme…) falls back to its first button.
const FALLBACK_CONTROL = 'button'

function findRow(id: string): HTMLElement | null {
  for (const el of document.querySelectorAll<HTMLElement>(`[data-settings-id="${id}"]`)) {
    if (el.getClientRects().length > 0) return el
  }
  return null
}

let clearTimer: ReturnType<typeof setTimeout> | null = null
let lit: HTMLElement | null = null

function unlight() {
  if (clearTimer) clearTimeout(clearTimer)
  clearTimer = null
  lit?.classList.remove(...HIT_CLASSES)
  lit = null
}

export function revealSettingsRow(id: string): void {
  let frames = 0
  const attempt = () => {
    const el = findRow(id)
    if (!el) {
      if (++frames < MAX_FRAMES) requestAnimationFrame(attempt)
      return
    }
    el.scrollIntoView({ block: 'center', behavior: 'smooth' })
    unlight()
    el.classList.add(...HIT_CLASSES)
    lit = el
    clearTimer = setTimeout(unlight, HIT_MS)
    const control = el.matches('button, input, select, textarea') ? el : el.querySelector<HTMLElement>(FIRST_CONTROL) ?? el.querySelector<HTMLElement>(FALLBACK_CONTROL)
    control?.focus({ preventScroll: true })
  }
  requestAnimationFrame(attempt)
}
