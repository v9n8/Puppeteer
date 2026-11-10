// The composer's permission mode / effort / response style chips (UX-26): what a
// session without a pick of its own resolves to, and the choices of the style
// chip. Pure, so tests cover them. A pick applies to the session's next start or
// resume; '' = no pick, and then the backend resolves the same defaults.
import { EFFORT_LEVELS, resolvePermissionMode, type EffortLevel } from './engineOptions.ts'
import { BUILTIN_STYLE_KEYS, isBuiltinStyle, type BuiltinStyleKey } from './responseStyles.ts'

export interface ComposerDefaults {
  /** Always a concrete mode ('default' = ask via UI). */
  permissionMode: string
  /** '' = the engine decides. */
  effort: string
  /** A style reference; '' = none. */
  style: string
}

interface DefaultSettings {
  default_permission_mode?: string | null
  default_effort?: string | null
  default_style?: string | null
}

/**
 * The values the chips show while the session has no pick of its own: the
 * global defaults (Settings › Defaults for new sessions). The one place the
 * composer reads defaults, so UX-19 adds the project tier here only, in the
 * backend's order (run pick > project > global).
 */
export function composerDefaults(src: { settings?: DefaultSettings | null }): ComposerDefaults {
  const s = src.settings
  const effort = s?.default_effort?.trim() ?? ''
  return {
    permissionMode: resolvePermissionMode('', s?.default_permission_mode),
    effort: (EFFORT_LEVELS as readonly string[]).includes(effort) ? effort : '',
    style: s?.default_style?.trim() ?? '',
  }
}

/**
 * The effort a launch sends: the pick when the next turn's model takes it (its
 * `levels`), else nothing, so the default applies instead of a level the
 * engine would drop.
 */
export function effortForLaunch(pick: string, levels: readonly EffortLevel[]): string | undefined {
  return pick && levels.includes(pick as EffortLevel) ? pick : undefined
}

/** The effort the next turn runs with, as the chip shows it. '' = the engine decides. */
export function effectiveEffort(pick: string, levels: readonly EffortLevel[], defaultEffort: string): string {
  return effortForLaunch(pick, levels) ?? (levels.includes(defaultEffort as EffortLevel) ? defaultEffort : '')
}

/**
 * The style a session without a pick runs its next turn with, mirroring the
 * backend's `resolve_run_style`: the run's stored style (a resume keeps it),
 * else the default; references that no longer resolve are skipped. `known`
 * tells whether a style id still exists (null while the list is not loaded:
 * every reference counts). 'default' (the built-in that adds no text) when
 * nothing resolves.
 */
export function inheritedStyle(
  runStyle: string | null | undefined,
  defaultStyle: string,
  known: ((id: string) => boolean) | null,
): string {
  for (const candidate of [runStyle, defaultStyle]) {
    const key = candidate?.trim()
    if (key && (isBuiltinStyle(key) || !known || known(key))) return key
  }
  return 'default'
}

export type ComposerStyleChoice =
  /** The built-in `default`: no extra text, so the chip calls it "None". */
  | { kind: 'none'; value: 'default' }
  | { kind: 'builtin'; value: BuiltinStyleKey }
  /** A global style (`mine`) or one of this project's (`project`). */
  | { kind: 'mine' | 'project'; value: string; name: string; description: string }
  /** The current value no longer resolves (a deleted style). */
  | { kind: 'missing'; value: string }

/**
 * The style chip's choices: None, the built-ins that add text, the user's
 * global styles, then this project's. Unlike the default-style setting, "None"
 * is the built-in `default`, so picking it overrides a stored or default style.
 * A `current` value that is none of these stays listed as `missing`.
 */
export function composerStyleChoices(
  styles: readonly { id: string; project_id: string | null; name: string; description: string }[],
  projectId: string | null | undefined,
  current: string | null | undefined,
): ComposerStyleChoice[] {
  const style = (kind: 'mine' | 'project') => (s: (typeof styles)[number]) =>
    ({ kind, value: s.id, name: s.name, description: s.description })
  const out: ComposerStyleChoice[] = [
    { kind: 'none', value: 'default' },
    ...BUILTIN_STYLE_KEYS.filter((k) => k !== 'default').map((k) => ({ kind: 'builtin' as const, value: k })),
    ...styles.filter((s) => s.project_id === null).map(style('mine')),
    ...(projectId ? styles.filter((s) => s.project_id === projectId).map(style('project')) : []),
  ]
  const cur = current?.trim()
  if (cur && !out.some((c) => c.value === cur)) out.push({ kind: 'missing', value: cur })
  return out
}
