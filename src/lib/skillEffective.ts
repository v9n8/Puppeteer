// "What the agent sees" (`list_effective_skills`): for one project, per engine,
// the skills an agent run there loads from its global (HOME) folder and the
// project's folder, read from the disk. The screen picks one engine and shows
// it in a Global and a Project section.

export const CMD_LIST_EFFECTIVE_SKILLS = 'list_effective_skills'

export type SkillScope = 'global' | 'project'
/** Puppeteer's link to a library skill, the repo's committed skill, or anything else. */
export type SkillOrigin = 'library' | 'git_managed' | 'unmanaged'

export const SKILL_ORIGINS: readonly SkillOrigin[] = ['library', 'git_managed', 'unmanaged']

export interface EffectiveSkill {
  name: string
  scope: SkillScope
  origin: SkillOrigin
  /** The library skill a `library` link points at. */
  skill_id: string | null
  /** A global skill of the same name wins, so the agent never loads this one. */
  shadowed: boolean
}

export interface EngineSkills {
  engine: { id: string; project_dir: string; home_dir: string | null }
  /** A global skill hides the project's of the same name (Claude); otherwise both load (Codex). */
  home_wins: boolean
  skills: EffectiveSkill[]
}

export interface EffectiveSection {
  scope: SkillScope
  /** The folder the section reads, as the user would type it. */
  folder: string
  /** Loaded ones first, then the shadowed ones; by name within each. */
  skills: EffectiveSkill[]
}

/**
 * The Global and Project sections of one engine (Global only when the engine
 * has a HOME folder), filtered by a case-insensitive name search. A section
 * the search empties is kept, so the folder stays visible.
 */
export function effectiveSections(view: EngineSkills | undefined, search = ''): EffectiveSection[] {
  if (!view) return []
  const q = search.trim().toLowerCase()
  const shown = view.skills.filter((s) => !q || s.name.toLowerCase().includes(q))
  const section = (scope: SkillScope, folder: string): EffectiveSection => ({
    scope,
    folder,
    skills: shown
      .filter((s) => s.scope === scope)
      .sort((a, b) => Number(a.shadowed) - Number(b.shadowed) || a.name.localeCompare(b.name)),
  })
  const out: EffectiveSection[] = []
  if (view.engine.home_dir) out.push(section('global', `~/${view.engine.home_dir}`))
  out.push(section('project', view.engine.project_dir))
  return out
}

/** How many skills the agent loads and how many are hidden, over the whole list. */
export function effectiveCounts(view: EngineSkills | undefined): { loaded: number; shadowed: number } {
  const skills = view?.skills ?? []
  const shadowed = skills.filter((s) => s.shadowed).length
  return { loaded: skills.length - shadowed, shadowed }
}
