// Pure helpers of the flag editors (V3-20): Settings › Flag now, Project › Flag in V3-21.
import { flagNameError, missingDefaultFlags, type SessionFlag } from './sessionFlags.ts'
import type { ProjectColor } from './projectAppearance.ts'

/** Why `name` cannot be saved (an i18n key), or null. `others`: the other flags of the same scope. */
export function validateFlagName(name: string, others: readonly Pick<SessionFlag, 'id' | 'name'>[]): string | null {
  return flagNameError(name, others)
}

/**
 * The i18n key of a backend refusal about the name (the commands return English
 * text), or null for any other message, which the caller shows as it is.
 */
export function backendNameErrorKey(message: string): string | null {
  if (/already exists/i.test(message)) return 'sessionFlags.errors.nameTaken'
  if (/longer than/i.test(message)) return 'sessionFlags.errors.nameTooLong'
  if (/needs a name/i.test(message)) return 'sessionFlags.errors.nameRequired'
  return null
}

/** The flags other than `exceptId` that already use `color` (the colour popover names them). */
export function colorUsers<F extends Pick<SessionFlag, 'id' | 'color'>>(
  flags: readonly F[],
  color: ProjectColor,
  exceptId: string | null = null,
): F[] {
  return flags.filter((f) => f.id !== exceptId && f.color === color)
}

/** Where the item at `index` lands after one arrow press (`-1` up, `1` down), kept inside the list. */
export function keyboardMove(ids: readonly string[], index: number, dir: -1 | 1): number {
  return Math.min(Math.max(index + dir, 0), ids.length - 1)
}

/** `ids` with the item at `from` moved to `to`. */
export function moveId(ids: readonly string[], from: number, to: number): string[] {
  const next = [...ids]
  const [id] = next.splice(from, 1)
  next.splice(to, 0, id)
  return next
}

/** True when no default flag is missing: "Restore default flags" has nothing to do. */
export function restoreDisabled(flags: readonly Pick<SessionFlag, 'builtin_key'>[]): boolean {
  return missingDefaultFlags(flags).length === 0
}

/** The second line of a shared flag in Project › Flag: how many Sessions carry it, or that it is hidden. */
export function sharedFlagSubline(flag: Pick<SessionFlag, 'hidden' | 'run_count'>, t: (key: string, params: { n: number }) => string): string {
  return t(flag.hidden ? 'sessionFlags.project.hiddenSub' : 'sessionFlags.project.inUse', { n: flag.run_count })
}
