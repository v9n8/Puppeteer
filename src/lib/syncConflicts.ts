// Pure helpers behind the Conflicts section of a Project (V3-47): the place
// badge, the order of the cards, and the sentence a re-check reports.
import type { ProjectSyncConflict, SyncScanReport } from '@/stores/projects'

/** Where the project's copy lives, for the "Skill · …" / "Rule · …" badge. */
export function conflictPlace(c: Pick<ProjectSyncConflict, 'kind' | 'engine' | 'block'>): string {
  if (c.kind === 'skill') return c.engine === 'codex' ? '.codex/skills' : '.claude/skills'
  if (c.engine === 'claude') return '.claude/rules'
  return c.block !== null ? 'AGENTS.md' : '.agents/skills'
}

/** Newest detection first; same time → by name, so the order never jumps. */
export function sortConflicts<T extends Pick<ProjectSyncConflict, 'detected_at' | 'name' | 'engine'>>(list: readonly T[]): T[] {
  return [...list].sort((a, b) =>
    b.detected_at.localeCompare(a.detected_at)
    || a.name.localeCompare(b.name)
    || a.engine.localeCompare(b.engine))
}

/** A long path cut in the middle ("/Users/me/…/SKILL.md"), keeping both ends readable. */
export function truncateMiddle(path: string, max = 56): string {
  if (path.length <= max || max < 5) return path
  const keep = max - 1
  const head = Math.ceil(keep / 2)
  return `${path.slice(0, head)}…${path.slice(path.length - (keep - head))}`
}

export interface ScanOutcome {
  /** Names still in conflict. */
  open: string[]
  /** Names that now match the central version. */
  synced: string[]
  errors: number
}

const unique = (names: string[]) => [...new Set(names)]

export function scanOutcome(report: SyncScanReport): ScanOutcome {
  return {
    open: unique(report.open.map((e) => e.name)),
    synced: unique(report.closed.filter((e) => e.reason === 'in_sync').map((e) => e.name)),
    errors: report.errors.length,
  }
}

type Translate = (key: string, params?: Record<string, unknown>) => string

/** The toast after "Check again", from what the scan really found. */
export function scanMessage(o: ScanOutcome, t: Translate): string {
  const parts = [
    o.open.length > 0
      ? t('projectDetail.scanStillOpen', { count: o.open.length, names: o.open.join(', ') })
      : t('projectDetail.scanNoneOpen'),
  ]
  if (o.synced.length > 0) parts.push(t('projectDetail.scanSynced', { names: o.synced.join(', ') }))
  if (o.errors > 0) parts.push(t('projectDetail.scanErrors', { count: o.errors }))
  return parts.join(' ')
}
