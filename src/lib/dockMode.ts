// Active runs dock: expanded list or one compact line. The choice is kept per
// window height: one for tall windows (default full) and one for short ones
// (default compact), so a low window never overwrites what the user picked at
// full height. Pure: no Vue or `@/` imports, so `node --test` can load it.

export type DockMode = 'full' | 'compact'

export interface DockPrefs {
  tall: DockMode
  short: DockMode
}

/** Windows lower than this (px) use the `short` choice. */
export const SHORT_HEIGHT = 800

/** Rows the expanded dock shows before it scrolls and offers "See all (N)". */
export const DOCK_MAX_ROWS = 4

export const DEFAULT_DOCK_PREFS: DockPrefs = { tall: 'full', short: 'compact' }

const isMode = (v: unknown): v is DockMode => v === 'full' || v === 'compact'

/** Reads the stored JSON. A bad or missing value falls back to the defaults; a single bad field only loses that field. */
export function parseDockPrefs(raw: string | null): DockPrefs {
  if (raw != null) {
    try {
      const v = JSON.parse(raw) as Partial<Record<keyof DockPrefs, unknown>> | null
      if (v && typeof v === 'object') {
        return {
          tall: isMode(v.tall) ? v.tall : DEFAULT_DOCK_PREFS.tall,
          short: isMode(v.short) ? v.short : DEFAULT_DOCK_PREFS.short,
        }
      }
    } catch {
      /* corrupt JSON: use the defaults */
    }
  }
  return { ...DEFAULT_DOCK_PREFS }
}

export function isShort(height: number): boolean {
  return height < SHORT_HEIGHT
}

export function dockModeFor(prefs: DockPrefs, height: number): DockMode {
  return isShort(height) ? prefs.short : prefs.tall
}

/** Flips only the current height's choice; the other one is untouched. */
export function toggledDockPrefs(prefs: DockPrefs, height: number): DockPrefs {
  const next: DockMode = dockModeFor(prefs, height) === 'full' ? 'compact' : 'full'
  return isShort(height) ? { ...prefs, short: next } : { ...prefs, tall: next }
}
