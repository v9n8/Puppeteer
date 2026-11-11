/**
 * The app's font and text sizes (Settings › General, E3–E5). Per-machine display
 * preferences kept in localStorage, like the row density, and applied as CSS
 * variables on <html>: `--font-ui` (the family), `--app-text-scale` and
 * `--chat-text-scale` (main.css scales the text sizes by them; spacing stays).
 * Every window applies them at boot and again when another window changes them.
 */

/** `theme` keeps the colour theme's own font (some themes swap it); `custom` uses `customFont`. */
export const FONT_FAMILIES = ['theme', 'inter', 'system', 'helvetica', 'noto', 'custom'] as const
export type FontFamilyId = (typeof FONT_FAMILIES)[number]

const FONT_STACKS: Record<Exclude<FontFamilyId, 'theme' | 'custom'>, string> = {
  inter: '"Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  system: 'system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  helvetica: '"Helvetica Neue", Helvetica, Arial, sans-serif',
  noto: '"Noto Sans", "Noto Sans JP", "Noto Sans CJK JP", sans-serif',
}

/** Text size of plain UI text (`text-xs`); the other sizes scale with it. */
export const APP_FONT_SIZES = [11, 12, 13, 14, 15, 16, 17, 18] as const
export const DEFAULT_APP_FONT_SIZE = 13
/** Text size of a conversation's answers (`.markdown-output`); the rest of the log scales with it. */
export const CHAT_FONT_SIZES = [11, 12, 13, 14, 15, 16, 17, 18, 19, 20] as const
export const DEFAULT_CHAT_FONT_SIZE = 14

export interface FontPrefs {
  family: FontFamilyId
  customFont: string
  appSize: number
  chatSize: number
}

export const DEFAULT_FONT_PREFS: FontPrefs = {
  family: 'theme',
  customFont: '',
  appSize: DEFAULT_APP_FONT_SIZE,
  chatSize: DEFAULT_CHAT_FONT_SIZE,
}

const STORAGE_KEY = 'puppeteer.fonts'

/** The `font-family` for a choice, or `null` to keep the theme's. A custom name falls back to Inter. */
export function fontStack(family: FontFamilyId, customFont: string): string | null {
  if (family === 'theme') return null
  if (family !== 'custom') return FONT_STACKS[family]
  // Quoted as one family name; characters that could end the declaration are dropped.
  const name = customFont.replace(/["\\;{}]/g, '').trim()
  return name ? `"${name}", ${FONT_STACKS.inter}` : null
}

/** A size from storage or the picker, or the default when it is not one of `sizes`. */
export function pickSize(value: unknown, sizes: readonly number[], fallback: number): number {
  const n = Number(value)
  return sizes.includes(n) ? n : fallback
}

/** Reads stored preferences; anything missing or unknown takes its default. */
export function parseFontPrefs(raw: string | null): FontPrefs {
  let v: Partial<Record<keyof FontPrefs, unknown>> = {}
  try {
    const parsed = raw ? JSON.parse(raw) : null
    if (parsed && typeof parsed === 'object') v = parsed
  } catch {
    /* corrupt value — defaults */
  }
  return {
    family: (FONT_FAMILIES as readonly unknown[]).includes(v.family) ? (v.family as FontFamilyId) : 'theme',
    customFont: typeof v.customFont === 'string' ? v.customFont : '',
    appSize: pickSize(v.appSize, APP_FONT_SIZES, DEFAULT_APP_FONT_SIZE),
    chatSize: pickSize(v.chatSize, CHAT_FONT_SIZES, DEFAULT_CHAT_FONT_SIZE),
  }
}

/** The CSS variables for `prefs`; `null` removes one (the stylesheet's value applies). */
export function fontCssVars(prefs: FontPrefs): Record<string, string | null> {
  const scale = (size: number, base: number) => (size === base ? null : String(Math.round((size / base) * 10000) / 10000))
  return {
    '--font-ui': fontStack(prefs.family, prefs.customFont),
    '--app-text-scale': scale(prefs.appSize, DEFAULT_APP_FONT_SIZE),
    '--chat-text-scale': scale(prefs.chatSize, DEFAULT_CHAT_FONT_SIZE),
  }
}

export function loadFontPrefs(): FontPrefs {
  try {
    return parseFontPrefs(localStorage.getItem(STORAGE_KEY))
  } catch {
    return { ...DEFAULT_FONT_PREFS }
  }
}

export function saveFontPrefs(prefs: FontPrefs): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs))
  } catch {
    /* storage unavailable — applied for this window only */
  }
}

export function applyFontPrefs(prefs: FontPrefs): void {
  const style = document.documentElement.style
  for (const [name, value] of Object.entries(fontCssVars(prefs))) {
    if (value === null) style.removeProperty(name)
    else style.setProperty(name, value)
  }
}

/** Applies the stored preferences now and whenever another window saves new ones. */
export function installFontPrefs(): void {
  applyFontPrefs(loadFontPrefs())
  window.addEventListener('storage', (e) => {
    if (e.key === STORAGE_KEY) applyFontPrefs(parseFontPrefs(e.newValue))
  })
}
