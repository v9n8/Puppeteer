// Theme, palette and language changed in Settings reach every window at once.
// Each window applies them from the settings row only when it loads (App.vue),
// so with Settings in its own window the main window and the other pop-outs
// would keep the old look until reopened. Settings (GeneralSettings) broadcasts
// the change; every window listens (installed in main.ts). In a remote-desktop
// window the event shim (lib/ipc/event.ts) keeps the message among the remote
// windows, so another machine's look never leaks into this machine's windows.
import { emit, listen } from '@tauri-apps/api/event'
import { normalizeColorTheme } from './colorThemes.ts'
import { resolveDark } from './themeMode.ts'

export const APPEARANCE_CHANGED = 'appearance:changed'

export interface AppearanceChange {
  /** 'light' | 'dark' | 'system' */
  theme?: string
  /** A palette id of lib/colorThemes. */
  colorTheme?: string
  /** 'en' | 'vi' */
  language?: string
}

/** The fields of a received payload that are strings; anything else is dropped. */
export function readAppearanceChange(payload: unknown): AppearanceChange {
  const out: AppearanceChange = {}
  if (!payload || typeof payload !== 'object') return out
  const p = payload as Record<string, unknown>
  for (const key of ['theme', 'colorTheme', 'language'] as const) {
    if (typeof p[key] === 'string') out[key] = p[key] as string
  }
  return out
}

/** The parts of <html> the look lives on (a fake one in tests). */
export interface AppearanceRoot {
  classList: { toggle(token: string, force?: boolean): boolean }
  setAttribute(name: string, value: string): void
  removeAttribute(name: string): void
}

/** Applies theme and palette to `root`, like App.vue does at start. `prefersDark` decides 'system'. */
export function applyAppearance(change: AppearanceChange, root: AppearanceRoot, prefersDark: boolean): void {
  if (change.theme !== undefined) {
    root.classList.toggle('dark', resolveDark(change.theme, prefersDark))
  }
  if (change.colorTheme !== undefined) {
    // The default (indigo) palette uses no attribute: it falls back to :root / .dark.
    const id = normalizeColorTheme(change.colorTheme)
    if (id !== 'default') root.setAttribute('data-theme', id)
    else root.removeAttribute('data-theme')
  }
}

function prefersDarkNow(): boolean {
  return window.matchMedia('(prefers-color-scheme: dark)').matches
}

/** Applies the change in this window, then tells the others. */
export function changeAppearance(change: AppearanceChange): void {
  applyAppearance(change, document.documentElement, prefersDarkNow())
  emit<AppearanceChange>(APPEARANCE_CHANGED, change).catch(() => {
    /* outside the Tauri shell: this window only */
  })
}

/** Follows changes made in any window. `setLocale` comes from i18n (kept out of this module's imports). */
export function installAppearanceSync(setLocale: (locale: string) => void): void {
  listen<unknown>(APPEARANCE_CHANGED, (e) => {
    const change = readAppearanceChange(e.payload)
    applyAppearance(change, document.documentElement, prefersDarkNow())
    if (change.language !== undefined) setLocale(change.language)
  }).catch(() => {
    /* outside the Tauri shell */
  })
}
