// Pure rules of the thumbnails on the "Open windows" cards (stores/openWindows,
// components/windows/WindowCard.vue). No Vue / Tauri / `@/` imports so `node --test`
// can load it (src/lib/__tests__/windowThumbs.test.mjs). The Rust side
// (commands/window_snapshot.rs) refuses the same labels.

export interface SnapshotResult {
  label: string
  image?: string | null
  error?: string | null
}

/** A thumbnail kept from an earlier shot. */
export interface ThumbEntry {
  src: string
  at: number
}

export type ThumbStatus = 'loading' | 'failed' | undefined

export type Thumb = { kind: 'image'; src: string } | { kind: 'icon' } | { kind: 'loading' }

/** The width asked of the shot: 2x the widest card (200px) is sharp enough on retina. */
export const THUMB_WIDTH = 400

/** The secret vault is never shot, neither here nor on a remote desktop (QĐ-18); nor the transient windows. */
export function isSnapshotAllowed(label: string): boolean {
  const own = label.startsWith('remote-') ? label.slice('remote-'.length) : label
  return (
    label !== 'main' &&
    label !== 'remote-bridge' &&
    own !== 'secret-notes' &&
    !own.startsWith('permission-')
  )
}

/**
 * What a card shows in its picture box. An earlier shot stays while a new one is taken (and
 * for a minimized window), so the picture never blinks; with none, the box is "loading" while
 * the shot is on its way and the big icon otherwise.
 */
export function thumbFor(
  card: { label: string },
  cache: Readonly<Record<string, ThumbEntry>>,
  status: ThumbStatus,
): Thumb {
  if (!isSnapshotAllowed(card.label)) return { kind: 'icon' }
  const hit = cache[card.label]
  if (hit) return { kind: 'image', src: hit.src }
  return status === 'loading' ? { kind: 'loading' } : { kind: 'icon' }
}

/** The banner shows when every window that could be shot failed; `unsupported` (Windows / Linux) is not a failure. */
export function shouldShowBanner(results: readonly SnapshotResult[]): boolean {
  const tried = results.filter((r) => r.error !== 'unsupported')
  return tried.length > 0 && tried.every((r) => !r.image)
}
