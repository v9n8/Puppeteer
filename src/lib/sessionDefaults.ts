// Settings › "Defaults for new sessions" (UX-18): the choices of its effort and
// response style selects, kept pure so tests can cover them.
import type { EffortLevel } from './engineOptions.ts'
import { NO_STYLE } from './responseStyles.ts'

export interface EffortChoice {
  /** '' = the engine decides. */
  value: string
  /** False for a saved level the default engine + model do not take. */
  supported: boolean
}

/**
 * The default effort select: "engine decides", then `levels` (what the default
 * engine + model take, from `effortLevelsFor`). A saved level outside them stays
 * listed, marked unsupported, so opening the screen never rewrites it: the key is
 * shared by both engines, and a run only applies a level its model takes.
 */
export function defaultEffortChoices(levels: readonly EffortLevel[], current: string | null | undefined): EffortChoice[] {
  const out: EffortChoice[] = [{ value: '', supported: true }, ...levels.map((l) => ({ value: l, supported: true }))]
  const saved = current?.trim()
  if (saved && !levels.includes(saved as EffortLevel)) out.push({ value: saved, supported: false })
  return out
}

export type StyleChoice =
  /** '' or a saved `default` ("No style"): both mean no style here. */
  | { kind: 'none'; value: '' | 'default' }
  | { kind: 'mine'; value: string; name: string; description: string }
  /** A saved reference that no longer resolves (a deleted style): runs ignore it. */
  | { kind: 'missing'; value: string }

/**
 * The default response style select (`default_style`): "None", then the global
 * styles (the samples included). A saved `default` means the same as '', so
 * "None" takes that value then and opening the screen never rewrites it. A
 * saved value that resolves to nothing stays listed as `missing`, so the
 * select never shows blank or rewrites it.
 */
export function defaultStyleChoices(
  styles: readonly { id: string; project_id: string | null; name: string; description: string }[],
  current: string | null | undefined,
): StyleChoice[] {
  const saved = current?.trim() ?? ''
  const out: StyleChoice[] = [
    { kind: 'none', value: saved === NO_STYLE ? NO_STYLE : '' },
    ...styles
      .filter((s) => s.project_id === null)
      .map((s) => ({ kind: 'mine' as const, value: s.id, name: s.name, description: s.description })),
  ]
  if (saved && !out.some((c) => c.value === saved)) out.push({ kind: 'missing', value: saved })
  return out
}
