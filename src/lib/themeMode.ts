// Light / dark / system resolved onto <html>. Shared by App.vue and the
// remote-desktop login window (which mounts without App.vue).

/** The part of <html> the mode lives on (a fake one in tests). */
export interface ThemeModeRoot {
  classList: { toggle(token: string, force?: boolean): boolean }
}

/** The part of a MediaQueryList this module uses. */
export interface ThemeModeMedia {
  matches: boolean
  addEventListener(type: 'change', listener: (e: { matches: boolean }) => void): void
  removeEventListener(type: 'change', listener: (e: { matches: boolean }) => void): void
}

/** 'dark' / 'light' are explicit; anything else (incl. an unknown value) follows the OS. */
export function resolveDark(theme: string, prefersDark: boolean): boolean {
  if (theme === 'dark') return true
  if (theme === 'light') return false
  return prefersDark
}

/**
 * Sets the `dark` class on `root`. With 'system' it also follows `media` through
 * exactly one listener. Returns the function that removes that listener.
 */
export function applyThemeMode(
  theme: string,
  root: ThemeModeRoot,
  media: ThemeModeMedia,
  onChange?: (dark: boolean) => void,
): () => void {
  const set = (dark: boolean) => {
    root.classList.toggle('dark', dark)
    onChange?.(dark)
  }
  set(resolveDark(theme, media.matches))
  if (theme === 'dark' || theme === 'light') return () => {}
  const listener = (e: { matches: boolean }) => set(e.matches)
  media.addEventListener('change', listener)
  return () => media.removeEventListener('change', listener)
}
