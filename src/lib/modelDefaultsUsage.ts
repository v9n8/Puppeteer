// Where a model is the default (Settings › Model, V3-63): the "in use" badges of a
// model row, and the place each `DefaultChange` of `save_enabled_models` names.
// The model a default moves to is NOT worked out here: the backend reports it
// (model_policy/save.rs), so the Save dialog shows exactly what will be written.

export type ModelEngine = 'claude' | 'codex'

/** A place that names a default model. */
export type ModelUsePlace = 'sessionDefault' | 'fallback' | 'translate' | 'focus' | 'project'

export interface ModelUse {
  place: ModelUsePlace
  projectId?: string
  projectName?: string
}

/** The settings keys read here (a subset of `get_settings`). */
export interface UsageSettings {
  claude_model?: string
  codex_model?: string
  claude_fallback_model?: string
  translate_engine?: string
  translate_model?: string
}

export interface UsageProject {
  id: string
  name: string
  session_defaults?: { model_claude?: string | null; model_codex?: string | null } | null
}

/** The engine a side task runs on when its setting is blank (as `save.rs::side_engine`). */
function sideEngine(engine: string | undefined): ModelEngine {
  return engine?.trim().toLowerCase() === 'codex' ? 'codex' : 'claude'
}

/** Model id → the places that use it, per engine. Blank values (the engine picks) name no model. */
export function modelDefaultsUsage(
  settings: UsageSettings | null | undefined,
  projects: readonly UsageProject[],
): Record<ModelEngine, Record<string, ModelUse[]>> {
  const out: Record<ModelEngine, Record<string, ModelUse[]>> = { claude: {}, codex: {} }
  const add = (engine: ModelEngine, id: string | null | undefined, use: ModelUse) => {
    const m = id?.trim()
    if (!m) return
    const list = (out[engine][m] ??= [])
    if (!list.some((u) => u.place === use.place && u.projectId === use.projectId)) list.push(use)
  }
  const s = settings ?? {}
  add('claude', s.claude_model, { place: 'sessionDefault' })
  add('codex', s.codex_model, { place: 'sessionDefault' })
  for (const id of (s.claude_fallback_model ?? '').split(',')) add('claude', id, { place: 'fallback' })
  add(sideEngine(s.translate_engine), s.translate_model, { place: 'translate' })
  for (const p of projects) {
    const use = { place: 'project' as const, projectId: p.id, projectName: p.name }
    add('claude', p.session_defaults?.model_claude, use)
    add('codex', p.session_defaults?.model_codex, use)
  }
  return out
}

/** One default `save_enabled_models` moves (model_policy/save.rs `DefaultChange`). */
export interface DefaultChange {
  /** The setting key, or `project.<column>`. */
  place: string
  from: string
  /** '' = the engine picks (a project then follows Settings). */
  to: string
  project_id?: string
  project_name?: string
}

/** Where a change lands in the UI: the place's kind and the Settings section that edits it, if any. */
export function defaultChangePlace(change: DefaultChange): { place: ModelUsePlace; section?: 'ai' | 'translate' } {
  switch (change.place) {
    case 'claude_model':
    case 'codex_model':
      return { place: 'sessionDefault', section: 'ai' }
    case 'claude_fallback_model':
      return { place: 'fallback', section: 'ai' }
    case 'translate_model':
      return { place: 'translate', section: 'translate' }
    case 'focus.ai_model':
      // No screen picks the Focus AI model (the setting is written directly).
      return { place: 'focus' }
    default:
      return { place: 'project' }
  }
}
