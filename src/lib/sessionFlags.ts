// Session flags (V3-18): types and pure helpers shared by the Session header and
// History (V3-19), Settings › Flag (V3-20), Project › Flag (V3-21) and the board
// (V3-22). The backend (commands/session_flags.rs) owns the rules: every flag is a
// row, shared (`project_id` null) or a project's own, and a project hides shared
// flags it does not use. The defaults are rows too, their id being their key.
import type { ProjectColor } from './projectAppearance.ts'

/** Longest flag name, as the backend checks it (counted in characters). */
export const FLAG_NAME_MAX = 24

/** The default flags, in their seeded order. */
export const DEFAULT_FLAG_KEYS = ['needs_review', 'in_progress', 'awaiting_review', 'settled'] as const
export type DefaultFlagKey = (typeof DEFAULT_FLAG_KEYS)[number]

export interface SessionFlag {
  id: string
  /** null: a shared flag, usable in every project. */
  project_id: string | null
  name: string
  /** A token of the project tab palette, so it follows the theme. */
  color: ProjectColor
  position: number
  /** The default this row is; null for a flag the user made. */
  builtin_key: DefaultFlagKey | null
  /** A default still named as seeded: the UI shows the translated name. */
  stock_name: boolean
  /** Listed for a project: a shared flag that project hides. */
  hidden: boolean
  /** Sessions outside the Trash with the flag (in the listed project, or everywhere). */
  run_count: number
}

export interface SessionFlagList {
  flags: SessionFlag[]
}

type FlagText = Pick<SessionFlag, 'name' | 'builtin_key' | 'stock_name'>

/** The name the UI shows: an untouched default's in the UI language, else the user's. */
export function flagLabel(flag: FlagText, t: (key: string) => string): string {
  return flag.builtin_key && flag.stock_name ? t(`sessionFlags.builtin.${flag.builtin_key}`) : flag.name
}

type MenuFlag = Pick<SessionFlag, 'project_id' | 'hidden'>

/** The flags a session's "Set flag" menu offers, shared then the project's own; hidden ones left out. */
export function flagMenuGroups<F extends MenuFlag>(flags: readonly F[]): { shared: F[]; project: F[] } {
  const shown = flags.filter((f) => !f.hidden)
  return { shared: shown.filter((f) => f.project_id === null), project: shown.filter((f) => f.project_id !== null) }
}

/** Ids a session of the listed project may be given (what `set_run_flag` accepts). */
export function usableFlagIds(flags: readonly Pick<SessionFlag, 'id' | 'hidden'>[]): Set<string> {
  return new Set(flags.filter((f) => !f.hidden).map((f) => f.id))
}

/** The defaults deleted from `flags`: what "Restore default flags" puts back (none: the button is off). */
export function missingDefaultFlags(flags: readonly Pick<SessionFlag, 'builtin_key'>[]): DefaultFlagKey[] {
  return DEFAULT_FLAG_KEYS.filter((k) => !flags.some((f) => f.builtin_key === k))
}

/** The flag with `id`, if listed (a session's `flag_id` may name a flag since deleted elsewhere). */
export function flagById<F extends Pick<SessionFlag, 'id'>>(flags: readonly F[], id: string | null | undefined): F | undefined {
  return id ? flags.find((f) => f.id === id) : undefined
}

/**
 * Why a flag name cannot be saved (an i18n key), or null when it can. `scope` is
 * the flags of the same scope (`label`: the name the UI shows, when it differs from
 * the stored one, as an untouched default's does); `exceptId` the flag being renamed.
 */
export function flagNameError(
  name: string,
  scope: readonly (Pick<SessionFlag, 'id' | 'name'> & { label?: string })[],
  exceptId: string | null = null,
): string | null {
  const trimmed = name.trim()
  if (!trimmed) return 'sessionFlags.errors.nameRequired'
  if ([...trimmed].length > FLAG_NAME_MAX) return 'sessionFlags.errors.nameTooLong'
  const lower = trimmed.toLowerCase()
  if (scope.some((f) => f.id !== exceptId && (f.name.toLowerCase() === lower || f.label?.toLowerCase() === lower))) return 'sessionFlags.errors.nameTaken'
  return null
}
