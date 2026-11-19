// Settings › "Defaults for new sessions" (UX-18): the choices of its effort,
// response style and sub-agent / worker selects, kept pure so tests can cover them.
import type { EffortLevel, SelectOption } from './engineOptions.ts'
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

/** What the sub-agent effort and worker effort selects store for "same as the session". */
export const INHERIT = 'inherit'
/** Sub-agent effort levels (QO-02): the aim is to lower cost, so none above High. */
export const SUBAGENT_EFFORT_LEVELS = ['low', 'medium', 'high'] as const

/**
 * The sub-agent model select (QO-01): "same as the session" (''), the `sonnet`
 * alias (the pinned table has no current Sonnet; the alias follows it), then the
 * choices of the "Claude model" select without its "engine default" entry.
 */
export function subagentModelChoices(
  claudeChoices: readonly SelectOption[],
  same: SelectOption,
  sonnet: SelectOption,
): SelectOption[] {
  return [same, sonnet, ...claudeChoices.filter((o) => o.value !== '' && o.value !== sonnet.value)]
}

/**
 * The sub-agent / worker effort selects (QO-02): `inherit` first, then `levels`.
 * A saved value outside them stays listed, so opening the screen never rewrites it.
 */
export function inheritEffortChoices(levels: readonly string[], current: string | null | undefined): string[] {
  const out = [INHERIT, ...levels]
  const saved = current?.trim()
  if (saved && !out.includes(saved)) out.push(saved)
  return out
}
