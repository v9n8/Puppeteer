// The composer's permission mode / effort / response style chips (UX-26): what a
// session without a pick of its own resolves to, and the choices of the style
// chip. Pure, so tests cover them. A pick applies to the session's next start or
// resume; '' = no pick, and then the backend resolves the same defaults.
import { EFFORT_LEVELS, resolvePermissionMode, type EffortLevel } from './engineOptions.ts'
import { NO_STYLE } from './responseStyles.ts'

export interface ComposerDefaults {
  /** The engine of a new session: 'claude' or 'codex'. */
  engine: string
  /** The default model of each engine; '' = the engine decides. */
  models: { claude: string; codex: string }
  /** Always a concrete mode ('default' = ask via UI). */
  permissionMode: string
  /** '' = the engine decides. */
  effort: string
  /** A style reference; '' = none. */
  style: string
  /**
   * The style references to try, project's first: a deleted one is skipped
   * and the next applies, as in the backend's `resolve_run_style`.
   */
  styles: string[]
}

interface DefaultSettings {
  default_engine?: string | null
  claude_model?: string | null
  codex_model?: string | null
  default_permission_mode?: string | null
  default_effort?: string | null
  default_style?: string | null
}

/** A project's defaults for new sessions (UX-19); null / '' = follow the global setting. */
export interface ProjectSessionDefaults {
  engine?: string | null
  model_claude?: string | null
  model_codex?: string | null
  permission_mode?: string | null
  effort?: string | null
  style?: string | null
}

const ENGINES = ['claude', 'codex']
const trimmed = (v: string | null | undefined) => v?.trim() ?? ''
const isEffort = (v: string) => (EFFORT_LEVELS as readonly string[]).includes(v)

/**
 * The values the composer shows while the session has no pick of its own: the
 * project's defaults, else the global ones (Settings › Defaults for new
 * sessions), field by field. The one place the composer reads defaults, in the
 * backend's order (run pick > the run's own > project > global).
 */
export function composerDefaults(src: {
  settings?: DefaultSettings | null
  project?: ProjectSessionDefaults | null
}): ComposerDefaults {
  const s = src.settings
  const p = src.project
  const pick = (project: string, global: string, ok: (v: string) => boolean = Boolean) =>
    ok(project) ? project : ok(global) ? global : ''
  const styles = [trimmed(p?.style), trimmed(s?.default_style)].filter(Boolean)
  return {
    engine: pick(trimmed(p?.engine), trimmed(s?.default_engine), (v) => ENGINES.includes(v)) || 'claude',
    models: {
      claude: pick(trimmed(p?.model_claude), trimmed(s?.claude_model)),
      codex: pick(trimmed(p?.model_codex), trimmed(s?.codex_model)),
    },
    permissionMode: resolvePermissionMode(trimmed(p?.permission_mode), s?.default_permission_mode),
    effort: pick(trimmed(p?.effort), trimmed(s?.default_effort), isEffort),
    style: styles[0] ?? '',
    styles,
  }
}

/**
 * The effort a session without a pick asks for, before its model's levels
 * apply: the level it last ran with (a resume keeps it), else the default.
 */
export function inheritedEffort(runEffort: string | null | undefined, defaultEffort: string): string {
  const own = trimmed(runEffort)
  return isEffort(own) ? own : defaultEffort
}

/**
 * The model a session without a pick runs on: the one it last ran with (a
 * resume keeps it) when that run is on `engine`, else the default of `engine`.
 */
export function inheritedModel(
  run: { engine?: string | null; model?: string | null } | null | undefined,
  engine: string,
  defaults: Pick<ComposerDefaults, 'models'>,
): string {
  const own = run?.engine === engine ? trimmed(run?.model) : ''
  return own || (engine === 'codex' ? defaults.models.codex : defaults.models.claude)
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
 * else the defaults in order (project's, then global); references that no
 * longer resolve are skipped. `known`
 * tells whether a style id still exists (null while the list is not loaded:
 * every reference counts). 'default' ("No style", which adds no text) when
 * nothing resolves.
 */
export function inheritedStyle(
  runStyle: string | null | undefined,
  defaultStyle: string | readonly string[],
  known: ((id: string) => boolean) | null,
): string {
  for (const candidate of [runStyle, ...(typeof defaultStyle === 'string' ? [defaultStyle] : defaultStyle)]) {
    const key = candidate?.trim()
    if (key && (key === NO_STYLE || !known || known(key))) return key
  }
  return NO_STYLE
}

export type ComposerStyleChoice =
  /** `default`: no extra text, so the chip calls it "None". */
  | { kind: 'none'; value: 'default' }
  /** A global style (`mine`, the samples included) or one of this project's (`project`). */
  | { kind: 'mine' | 'project'; value: string; name: string; description: string }
  /** The current value no longer resolves (a deleted style). */
  | { kind: 'missing'; value: string }

/**
 * The style chip's choices: None, the global styles, then this project's.
 * Unlike the default-style setting, "None" is `default`, so picking it
 * overrides a stored or default style.
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
    { kind: 'none', value: NO_STYLE },
    ...styles.filter((s) => s.project_id === null).map(style('mine')),
    ...(projectId ? styles.filter((s) => s.project_id === projectId).map(style('project')) : []),
  ]
  const cur = current?.trim()
  if (cur && !out.some((c) => c.value === cur)) out.push({ kind: 'missing', value: cur })
  return out
}
