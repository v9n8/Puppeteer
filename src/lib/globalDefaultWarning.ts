// Saving a global engine or default model (Settings › Defaults for new sessions)
// carries every project that follows it; a project whose own model list
// (Project › Model) turns the new model off would then refuse a new session.
// Pure, so tests cover it.
import { composerDefaults, type DefaultSettings, type ProjectSessionDefaults } from './composerChips.ts'
import { isModelOff, type EnabledModels } from './enabledModels.ts'

const ENGINES = ['claude', 'codex']

/** The settings keys whose change can block a following project. */
export const FOLLOWED_MODEL_KEYS = ['default_engine', 'claude_model', 'codex_model']

interface ProjectRow {
  id: string
  name: string
  session_defaults?: ProjectSessionDefaults | null
}

/**
 * Projects that take their engine or that engine's model from the global
 * default and whose resolved (engine, model) is off in their own list, so a new
 * session there is refused. `enabledFor` gives a project's list (Settings'
 * narrowed by its own). Projects in the Trash are not passed in.
 */
export function blockedFollowers(
  projects: readonly ProjectRow[],
  settings: DefaultSettings,
  enabledFor: (projectId: string) => EnabledModels,
): { id: string; name: string }[] {
  return projects
    .filter((p) => {
      const own = p.session_defaults
      const d = composerDefaults({ settings, project: own })
      const engine = d.engine as 'claude' | 'codex'
      const followsEngine = !ENGINES.includes(own?.engine?.trim() ?? '')
      const ownModel = engine === 'codex' ? own?.model_codex : own?.model_claude
      const followsModel = !ownModel?.trim()
      return (followsEngine || followsModel) && isModelOff(engine, d.models[engine], enabledFor(p.id))
    })
    .map((p) => ({ id: p.id, name: p.name }))
}

/** Up to `max` names joined with ", ", and how many are left out. */
export function namesShown(names: readonly string[], max = 3): { names: string; more: number } {
  return { names: names.slice(0, max).join(', '), more: Math.max(0, names.length - max) }
}
