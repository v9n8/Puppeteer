// Response styles (UX-17, UX-36): types and pure helpers shared by Settings › Response
// styles and the session pickers. The backend (commands/response_styles.rs) owns
// the resolution order. Every style is a row referenced by id, the samples too
// (their id is their key); only `default` ("No style") lives in code.

/** The style that adds no text ("No style"): not a row, never deleted. */
export const NO_STYLE = 'default'

/** The sample styles, in display order (rows whose id and `builtin_key` are the key). */
export const SAMPLE_STYLE_KEYS = ['concise', 'explain_junior', 'code_review'] as const
export type SampleStyleKey = (typeof SAMPLE_STYLE_KEYS)[number]

/** Same limits as the backend's validation. */
export const STYLE_NAME_MAX = 80
export const STYLE_DESCRIPTION_MAX = 300
export const STYLE_BODY_MAX = 8000

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
  /** The sample this row is; null for the user's own styles. */
  builtin_key: SampleStyleKey | null
  /** A sample whose name is still the seeded English one: shown translated. */
  stock_name: boolean
  /** Same for the description. */
  stock_description: boolean
}

export interface ResponseStyleList {
  styles: ResponseStyle[]
}

type StyleText = Pick<ResponseStyle, 'name' | 'description' | 'builtin_key' | 'stock_name' | 'stock_description'>

/**
 * The name and description the UI shows: a sample's untouched ones in the UI
 * language, else what the user typed.
 */
export function styleText(s: StyleText, t: (key: string) => string): { name: string; description: string } {
  const key = s.builtin_key
  return {
    name: key && s.stock_name ? t(`responseStyles.builtin.${key}.name`) : s.name,
    description: key && s.stock_description ? t(`responseStyles.builtin.${key}.description`) : s.description,
  }
}

/** `styles` with the shown name and description, for the pickers. */
export function shownStyles<S extends StyleText>(styles: readonly S[], t: (key: string) => string): S[] {
  return styles.map((s) => ({ ...s, ...styleText(s, t) }))
}

/** The samples deleted from `styles`: what "Restore sample styles" puts back. */
export function missingSamples(styles: readonly Pick<ResponseStyle, 'builtin_key'>[]): SampleStyleKey[] {
  return SAMPLE_STYLE_KEYS.filter((k) => !styles.some((s) => s.builtin_key === k))
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
