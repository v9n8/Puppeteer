// Named color palettes (Settings → Color theme). Each id matches a
// `[data-theme='<id>']` block in src/assets/themes/<id>.css; `default` has no
// attribute (the :root / .dark tokens in main.css). Pure data, shared by the
// desktop picker and the phone Controller.

export const COLOR_THEMES = [
  'default',
  'contrast',
  'graphite',
  'ocean',
  'paper',
  'forest',
  'nord',
  'tokyonight',
] as const

export type ColorThemeId = (typeof COLOR_THEMES)[number]

// Removed palettes still stored in settings or on a phone: read as the closest one left.
const REPLACED_BY: Record<string, ColorThemeId> = {
  macos: 'ocean',
  tahoe: 'ocean',
  crt: 'contrast',
  monokai: 'tokyonight',
  midautumn: 'default',
}

/** A stored palette id as one that still exists, or null when it was never a palette. */
export function knownColorTheme(id: unknown): ColorThemeId | null {
  if (typeof id !== 'string') return null
  if ((COLOR_THEMES as readonly string[]).includes(id)) return id as ColorThemeId
  return REPLACED_BY[id] ?? null
}

/** A stored palette id as one that still exists; unknown ids become `default`. */
export function normalizeColorTheme(id: unknown): ColorThemeId {
  return knownColorTheme(id) ?? 'default'
}

function capitalized(id: string): string {
  return `${id.charAt(0).toUpperCase()}${id.slice(1)}`
}

/** i18n key of a palette's name, e.g. `tokyonight` → `settings.general.colorTokyonight`. */
export function colorThemeLabelKey(id: string): string {
  return `settings.general.color${capitalized(id)}`
}

/** i18n key of a palette's one-line description, e.g. `settings.general.colorTokyonightHint`. */
export function colorThemeHintKey(id: string): string {
  return `${colorThemeLabelKey(id)}Hint`
}
