// Full sidebar or icon rail. The choice is kept per window size: one for wide
// windows (default full) and one for narrow ones (default rail), so shrinking
// the window never overwrites what the user picked at full size. Pure: no Vue
// or `@/` imports, so `node --test` can load it.

export type SidebarMode = 'full' | 'rail'

export interface SidebarPrefs {
  wide: SidebarMode
  narrow: SidebarMode
}

/** Windows narrower than this (px) use the `narrow` choice. */
export const NARROW_WIDTH = 1100

export const DEFAULT_PREFS: SidebarPrefs = { wide: 'full', narrow: 'rail' }

const isMode = (v: unknown): v is SidebarMode => v === 'full' || v === 'rail'

/**
 * Reads the stored JSON. A bad or missing value falls back to the defaults, a
 * single bad field only loses that field. `legacyHidden` is the old
 * `puppeteer.sidebarHidden` key: '1' (hidden outright) becomes rail at both sizes.
 */
export function parseSidebarPrefs(raw: string | null, legacyHidden: string | null = null): SidebarPrefs {
  if (raw != null) {
    try {
      const v = JSON.parse(raw) as Partial<Record<keyof SidebarPrefs, unknown>> | null
      if (v && typeof v === 'object') {
        return {
          wide: isMode(v.wide) ? v.wide : DEFAULT_PREFS.wide,
          narrow: isMode(v.narrow) ? v.narrow : DEFAULT_PREFS.narrow,
        }
      }
    } catch {
      /* corrupt JSON: use the defaults */
    }
    return { ...DEFAULT_PREFS }
  }
  if (legacyHidden === '1') return { wide: 'rail', narrow: 'rail' }
  return { ...DEFAULT_PREFS }
}

export function isNarrow(width: number): boolean {
  return width < NARROW_WIDTH
}

export function sidebarModeFor(prefs: SidebarPrefs, width: number): SidebarMode {
  return isNarrow(width) ? prefs.narrow : prefs.wide
}

/** `prefs` with the current size's choice set to `mode`; the other size is untouched. */
export function withMode(prefs: SidebarPrefs, width: number, mode: SidebarMode): SidebarPrefs {
  return isNarrow(width) ? { ...prefs, narrow: mode } : { ...prefs, wide: mode }
}

/** Flips only the current size's choice. */
export function toggledPrefs(prefs: SidebarPrefs, width: number): SidebarPrefs {
  return withMode(prefs, width, sidebarModeFor(prefs, width) === 'full' ? 'rail' : 'full')
}
