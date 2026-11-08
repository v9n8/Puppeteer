// Bulk switch of the Skills Allocation matrix (`plan_skill_bulk` /
// `apply_skill_bulk`): the request, the preview the backend returns, and the
// pure helpers the preview dialog shows it with.

export interface SkillBulkRequest {
  skill_ids: string[]
  /** Adds every member of this skill group (`library.groups`). */
  group_id: string | null
  project_ids: string[]
  engines: string[]
  on: boolean
}

export interface BulkFileChange {
  action: 'create' | 'remove'
  path: string
}

export interface BulkChange {
  project_id: string
  project_name: string
  skill_id: string
  skill_name: string
  engines: string[]
  /** Empty when only the switch changes. */
  files: BulkFileChange[]
}

export type BulkSkipReason = 'git_managed' | 'occupied' | 'project_missing' | 'source_missing'

export interface BulkSkip {
  project_id: string
  project_name: string
  skill_id: string
  skill_name: string
  /** null: the whole project is skipped for that skill. */
  engine: string | null
  reason: BulkSkipReason
  path: string | null
}

export interface SkillBulkPlan {
  changes: BulkChange[]
  skipped: BulkSkip[]
  unchanged: number
}

// A type, not an interface: it is passed as the named values of an i18n message.
export type BulkSummary = {
  changes: number
  create: number
  remove: number
  skipped: number
  unchanged: number
}

export function summarizeBulkPlan(plan: SkillBulkPlan): BulkSummary {
  const files = plan.changes.flatMap((c) => c.files)
  return {
    changes: plan.changes.length,
    create: files.filter((f) => f.action === 'create').length,
    remove: files.filter((f) => f.action === 'remove').length,
    skipped: plan.skipped.length,
    unchanged: plan.unchanged,
  }
}

export interface BulkProjectSection {
  projectId: string
  projectName: string
  changes: BulkChange[]
  skipped: BulkSkip[]
}

/** Changes and skips by project, in the order the backend listed the projects. */
export function bulkPlanByProject(plan: SkillBulkPlan): BulkProjectSection[] {
  const sections = new Map<string, BulkProjectSection>()
  const section = (id: string, name: string) => {
    let s = sections.get(id)
    if (!s) sections.set(id, (s = { projectId: id, projectName: name, changes: [], skipped: [] }))
    return s
  }
  for (const c of plan.changes) section(c.project_id, c.project_name).changes.push(c)
  for (const s of plan.skipped) section(s.project_id, s.project_name).skipped.push(s)
  return [...sections.values()]
}

/** Whether the form picks anything to plan: some skill (or a group), project and engine. */
export function bulkRequestReady(req: SkillBulkRequest): boolean {
  return (req.skill_ids.length > 0 || !!req.group_id) && req.project_ids.length > 0 && req.engines.length > 0
}

/** `list` with `id` added or dropped, keeping the order of `all`. */
export function toggleId(list: readonly string[], id: string, all: readonly string[]): string[] {
  const next = new Set(list)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  return all.filter((x) => next.has(x))
}
