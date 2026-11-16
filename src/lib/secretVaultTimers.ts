// Pure timers of the Secret notes screen (kept free of Vue / Tauri imports so
// node:test can load them): the "auto-hide in N s" countdown and the auto-lock menu.

/** The auto-lock menu: minutes the vault may sit idle before it locks. */
export const AUTO_LOCK_CHOICES: readonly number[] = [1, 5, 15, 30]
export const DEFAULT_AUTO_LOCK_MINUTES = 5

/** Whole seconds left before a content revealed at `revealedAt` (ms) hides; never below 0. */
export function revealCountdown(revealedAt: number, now: number, secs: number): number {
  return Math.max(0, Math.ceil((revealedAt + secs * 1000 - now) / 1000))
}

/** A stored / reported value as a menu choice; anything else (missing, odd, off the menu) is the default. */
export function parseAutoLockMinutes(raw: unknown): number {
  const n = typeof raw === 'string' ? Number(raw.trim()) : raw
  return typeof n === 'number' && AUTO_LOCK_CHOICES.includes(n) ? n : DEFAULT_AUTO_LOCK_MINUTES
}
