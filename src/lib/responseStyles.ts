// Response styles (UX-17): types and pure helpers shared by Settings › Response
// styles and the session pickers. The backend (commands/response_styles.rs) owns
// the built-in texts and the resolution order; a style is referenced by a
// built-in key or by the id of one of the user's styles.

/** Built-in style keys, in display order. `default` adds no text. */
export const BUILTIN_STYLE_KEYS = ['default', 'concise', 'explain_junior', 'code_review'] as const
export type BuiltinStyleKey = (typeof BUILTIN_STYLE_KEYS)[number]

/** Same limits as the backend's validation. */
export const STYLE_NAME_MAX = 80
export const STYLE_DESCRIPTION_MAX = 300
export const STYLE_BODY_MAX = 8000

export interface BuiltinStyle {
  key: BuiltinStyleKey
  body: string
}

export interface ResponseStyle {
  id: string
  /** null: a global style. */
  project_id: string | null
  name: string
  description: string
  body: string
  position: number
  created_at: string
  updated_at: string
}

export interface ResponseStyleList {
  builtins: BuiltinStyle[]
  styles: ResponseStyle[]
}

export function isBuiltinStyle(key: string | null | undefined): key is BuiltinStyleKey {
  return !!key && (BUILTIN_STYLE_KEYS as readonly string[]).includes(key)
}

/** i18n key of a built-in style's display name. */
export function builtinStyleLabelKey(key: BuiltinStyleKey): string {
  return `responseStyles.builtin.${key}.name`
}

/**
 * `name`, or `name (2)`, `name (3)`, … — the first one no style in `taken`
 * already uses (names are unique per scope, case-insensitively). Used when a
 * built-in is copied to "My styles".
 */
export function uniqueStyleName(name: string, taken: readonly string[]): string {
  const base = name.trim()
  const used = new Set(taken.map((n) => n.trim().toLowerCase()))
  if (!used.has(base.toLowerCase())) return base
  for (let n = 2; ; n++) {
    const candidate = `${base} (${n})`
    if (!used.has(candidate.toLowerCase())) return candidate
  }
}

/** Why a style form cannot be saved (an i18n key), or null when it can. */
export function styleFormError(form: { name: string; description: string; body: string }): string | null {
  const name = form.name.trim()
  const body = form.body.trim()
  if (!name) return 'responseStyles.errors.nameRequired'
  if ([...name].length > STYLE_NAME_MAX) return 'responseStyles.errors.nameTooLong'
  if ([...form.description.trim()].length > STYLE_DESCRIPTION_MAX) return 'responseStyles.errors.descriptionTooLong'
  if (!body) return 'responseStyles.errors.bodyRequired'
  if ([...body].length > STYLE_BODY_MAX) return 'responseStyles.errors.bodyTooLong'
  return null
}
