// Skill Doctor (`diagnose_skills`, gated by `library.skill_doctor`): the
// report the backend returns and the pure helpers the skill detail and the
// Allocation matrix show it with. An issue is a rule id + params; the sentence
// comes from i18n (`skillDoctor.rule.<rule>`).

export const CMD_DIAGNOSE_SKILLS = 'diagnose_skills'
export const SKILL_DOCTOR_FEATURE = 'library.skill_doctor'

export type DoctorSeverity = 'error' | 'warn' | 'info'

/** Worst first: the order groups and badges follow. */
export const DOCTOR_SEVERITIES: readonly DoctorSeverity[] = ['error', 'warn', 'info']

/** Every rule the backend reports (`skill_doctor.rs`, enum `Rule`). */
export const DOCTOR_RULES = [
  'unreadable',
  'missing-name',
  'missing-description',
  'invalid-id',
  'name-id-mismatch',
  'unknown-frontmatter-key',
  'description-too-short',
  'description-too-long',
  'description-no-trigger',
  'shadowed-id',
  'duplicate-name',
  'body-empty',
  'body-thin',
  'body-too-large',
  'broken-link',
  'link-outside-folder',
  'oversized-asset',
] as const

export interface DoctorIssue {
  /** Not limited to `DOCTOR_RULES`: a newer backend may send a rule this UI doesn't know yet. */
  rule: string
  severity: DoctorSeverity
  params?: Record<string, string | number>
}

export interface SkillDoctorReport {
  skill_id: string
  name: string
  /** Unix milliseconds. */
  checked_at: number
  issues: DoctorIssue[]
  stats: { file_bytes: number; body_chars: number; description_chars: number; asset_files: number }
}

export interface DoctorGroup {
  severity: DoctorSeverity
  issues: DoctorIssue[]
}

/** Issues by severity, worst first; empty severities are left out. */
export function groupIssuesBySeverity(issues: readonly DoctorIssue[]): DoctorGroup[] {
  return DOCTOR_SEVERITIES.map((severity) => ({ severity, issues: issues.filter((i) => i.severity === severity) }))
    .filter((g) => g.issues.length > 0)
}

export function countBySeverity(issues: readonly DoctorIssue[]): Record<DoctorSeverity, number> {
  const counts: Record<DoctorSeverity, number> = { error: 0, warn: 0, info: 0 }
  for (const i of issues) if (i.severity in counts) counts[i.severity]++
  return counts
}

/** The severities a matrix badge shows, worst first. */
export const BADGE_SEVERITIES = ['error', 'warn'] as const
export type BadgeSeverity = (typeof BADGE_SEVERITIES)[number]

export interface DoctorBadge {
  /** The worst one. */
  severity: BadgeSeverity
  /** Issues of that severity. */
  count: number
  counts: Record<BadgeSeverity, number>
}

/**
 * The matrix badge of one skill: its worst severity and how many issues have
 * it. Notes alone get none — `unknown-frontmatter-key` fires on keys Claude
 * Code reads but the Doctor doesn't know yet, and a badge on most rows would
 * hide the ones that matter. The detail still lists them.
 */
export function doctorBadge(report: SkillDoctorReport | undefined): DoctorBadge | null {
  if (!report) return null
  const { error, warn } = countBySeverity(report.issues)
  const counts = { error, warn }
  const severity = BADGE_SEVERITIES.find((s) => counts[s] > 0)
  return severity ? { severity, count: counts[severity], counts } : null
}

/** Reports keyed by skill id, for the matrix rows. */
export function reportsById(reports: readonly SkillDoctorReport[]): Record<string, SkillDoctorReport> {
  return Object.fromEntries(reports.map((r) => [r.skill_id, r]))
}
