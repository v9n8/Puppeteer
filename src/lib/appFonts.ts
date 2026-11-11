/**
 * The app's fonts and text sizes (Settings › General). Per-machine display
 * preferences kept in localStorage, like the row density, and applied on <html>:
 * CSS variables `--font-ui` (the family), `--font-mono` (code), the
 * `--font-weight-*` scale, `--app-text-scale` and `--chat-text-scale` (main.css
 * scales the text sizes by them; spacing stays), plus `data-font-smoothing`.
 * Every window applies them at boot and again when another window changes them.
 */

/** `custom` uses `customFont`. */
export const FONT_FAMILIES = ['inter', 'bevietnam', 'plex', 'noto', 'system', 'custom'] as const
export type FontFamilyId = (typeof FONT_FAMILIES)[number]

// Choices of earlier versions: themes no longer swap the font, and Helvetica is
// the system font on a Mac (Arial on Windows).
const LEGACY_FAMILIES: Record<string, FontFamilyId> = { theme: 'inter', helvetica: 'system' }

const INTER_STACK = '"Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif'
const FONT_STACKS: Record<Exclude<FontFamilyId, 'custom'>, string> = {
  inter: INTER_STACK,
  bevietnam: `"Be Vietnam Pro", ${INTER_STACK}`,
  plex: `"IBM Plex Sans", ${INTER_STACK}`,
  noto: '"Noto Sans", "Noto Sans JP", "Noto Sans CJK JP", sans-serif',
  system: 'system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
}

/** Weight of plain text; medium, semibold and bold move by the same step. */
export const FONT_WEIGHTS = [400, 450, 500] as const
export type FontWeight = (typeof FONT_WEIGHTS)[number]
/** Bundled with fixed weights only (400, 500…), so 450 would just round down. */
export const STATIC_WEIGHT_FAMILIES: readonly FontFamilyId[] = ['bevietnam']

/**
 * `crisp` keeps the stem thickness macOS draws (best on a 1x panel), `thin` is
 * grayscale antialiasing (best on Retina), `auto` picks by the screen's density.
 * Windows and Linux ignore it (ClearType / FreeType).
 */
export const FONT_SMOOTHINGS = ['auto', 'crisp', 'thin'] as const
export type FontSmoothing = (typeof FONT_SMOOTHINGS)[number]

export const MONO_FONTS = ['jetbrains', 'plexmono', 'system'] as const
export type MonoFontId = (typeof MONO_FONTS)[number]
const MONO_STACKS: Record<MonoFontId, string> = {
  jetbrains: '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
  plexmono: '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
  system: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
}

/** Text size of plain UI text (`text-xs`); the other sizes scale with it. 13 px is the floor. */
export const APP_FONT_SIZES = [13, 14, 15, 16, 17, 18] as const
export const DEFAULT_APP_FONT_SIZE = 13
/** Text size of a conversation's answers (`.markdown-output`); the rest of the log scales with it. */
export const CHAT_FONT_SIZES = [13, 14, 15, 16, 17, 18, 19, 20] as const
export const DEFAULT_CHAT_FONT_SIZE = 14

export interface FontPrefs {
  family: FontFamilyId
  customFont: string
  weight: FontWeight
  smoothing: FontSmoothing
  mono: MonoFontId
  appSize: number
  chatSize: number
}

export const DEFAULT_FONT_PREFS: FontPrefs = {
  family: 'inter',
  customFont: '',
  weight: 400,
  smoothing: 'auto',
  mono: 'jetbrains',
  appSize: DEFAULT_APP_FONT_SIZE,
  chatSize: DEFAULT_CHAT_FONT_SIZE,
}

const STORAGE_KEY = 'puppeteer.fonts'

/** The `font-family` for a choice, or `null` to keep the stylesheet's (Inter). A custom name falls back to Inter. */
export function fontStack(family: FontFamilyId, customFont: string): string | null {
  if (family === 'inter') return null
  if (family !== 'custom') return FONT_STACKS[family]
  // Quoted as one family name; characters that could end the declaration are dropped.
  const name = customFont.replace(/["\\;{}]/g, '').trim()
  return name ? `"${name}", ${INTER_STACK}` : null
}

/** The stack a family's sample is drawn in (the picker); `custom` has none. */
export function familyStack(family: FontFamilyId): string | null {
  return family === 'custom' ? null : FONT_STACKS[family]
}

/** Whether the family can draw `weight` (the in-between 450 needs a weight axis). */
export function supportsWeight(family: FontFamilyId, weight: FontWeight): boolean {
  return weight !== 450 || !STATIC_WEIGHT_FAMILIES.includes(family)
}

/** A size from storage or the picker; below the list it rises to the smallest size, otherwise unknown → `fallback`. */
export function pickSize(value: unknown, sizes: readonly number[], fallback: number): number {
  const n = Number(value)
  if (sizes.includes(n)) return n
  return Number.isFinite(n) && n > 0 && n < sizes[0] ? sizes[0] : fallback
}

function pick<T>(value: unknown, allowed: readonly T[], fallback: T): T {
  return allowed.includes(value as T) ? (value as T) : fallback
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
  const family = typeof v.family === 'string' && v.family in LEGACY_FAMILIES
    ? LEGACY_FAMILIES[v.family]
    : pick(v.family, FONT_FAMILIES, DEFAULT_FONT_PREFS.family)
  const weight = pick(v.weight, FONT_WEIGHTS, DEFAULT_FONT_PREFS.weight)
  return {
    family,
    customFont: typeof v.customFont === 'string' ? v.customFont : '',
    weight: supportsWeight(family, weight) ? weight : 400,
    smoothing: pick(v.smoothing, FONT_SMOOTHINGS, DEFAULT_FONT_PREFS.smoothing),
    mono: pick(v.mono, MONO_FONTS, DEFAULT_FONT_PREFS.mono),
    appSize: pickSize(v.appSize, APP_FONT_SIZES, DEFAULT_APP_FONT_SIZE),
    chatSize: pickSize(v.chatSize, CHAT_FONT_SIZES, DEFAULT_CHAT_FONT_SIZE),
  }
}

/** The CSS variables for `prefs`; `null` removes one (the stylesheet's value applies). */
export function fontCssVars(prefs: FontPrefs): Record<string, string | null> {
  const scale = (size: number, base: number) => (size === base ? null : String(Math.round((size / base) * 10000) / 10000))
  const step = prefs.weight - 400
  const weight = (base: number) => (step ? String(base + step) : null)
  return {
    '--font-ui': fontStack(prefs.family, prefs.customFont),
    '--font-mono': prefs.mono === DEFAULT_FONT_PREFS.mono ? null : MONO_STACKS[prefs.mono],
    '--font-weight-normal': weight(400),
    '--font-weight-medium': weight(500),
    '--font-weight-semibold': weight(600),
    '--font-weight-bold': weight(700),
    '--app-text-scale': scale(prefs.appSize, DEFAULT_APP_FONT_SIZE),
    '--chat-text-scale': scale(prefs.chatSize, DEFAULT_CHAT_FONT_SIZE),
  }
}

/** What `smoothing` draws on a screen of `pixelRatio`. */
export function resolveSmoothing(smoothing: FontSmoothing, pixelRatio: number): 'crisp' | 'thin' {
  if (smoothing !== 'auto') return smoothing
  return pixelRatio >= 2 ? 'thin' : 'crisp'
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

let applied: FontPrefs = DEFAULT_FONT_PREFS

export function applyFontPrefs(prefs: FontPrefs): void {
  applied = prefs
  const root = document.documentElement
  for (const [name, value] of Object.entries(fontCssVars(prefs))) {
    if (value === null) root.style.removeProperty(name)
    else root.style.setProperty(name, value)
  }
  root.dataset.fontSmoothing = resolveSmoothing(prefs.smoothing, window.devicePixelRatio)
}

/** Applies the stored preferences now, when another window saves new ones, and when the window moves to a screen of another density. */
export function installFontPrefs(): void {
  applyFontPrefs(loadFontPrefs())
  window.addEventListener('storage', (e) => {
    if (e.key === STORAGE_KEY) applyFontPrefs(parseFontPrefs(e.newValue))
  })
  window.matchMedia('(min-resolution: 2dppx)').addEventListener('change', () => applyFontPrefs(applied))
}
