// Schedules screen (`orchestration.schedules`): the shapes of src-tauri/src/schedules
// and the editor form, as pure functions (tested in __tests__/schedules.test.mjs).

export const CMD_LIST_SCHEDULES = 'list_schedules'
export const CMD_SAVE_SCHEDULE = 'save_schedule'
export const CMD_DELETE_SCHEDULE = 'delete_schedule'
/** Local-only: a remote desktop cannot switch a schedule on. */
export const CMD_SET_SCHEDULE_ENABLED = 'set_schedule_enabled'
export const EVT_SCHEDULES_CHANGED = 'schedules:changed'

export const MAX_INTERVAL_MINUTES = 7 * 24 * 60
/** Same order as `schedules::PERMISSION_MODES`; `plan` is the default. */
export const PERMISSION_MODES = ['plan', 'auto', 'default', 'acceptEdits', 'bypassPermissions'] as const
export const TIMING_KINDS = ['interval', 'daily', 'weekly', 'once'] as const

export type PermissionMode = (typeof PERMISSION_MODES)[number]
export type TimingKind = (typeof TIMING_KINDS)[number]

export type Timing =
  | { kind: 'interval'; every_minutes: number }
  | { kind: 'daily'; time: string }
  | { kind: 'weekly'; weekdays: number[]; time: string }
  | { kind: 'once'; at: string }

export interface Schedule {
  id: string
  project_id: string
  name: string
  prompt: string
  engine: string | null
  permission_mode: string
  /** null: the stored timing does not parse (never due). */
  timing: Timing | null
  enabled: boolean
  /** Proposed by an agent and not reviewed yet: it cannot be switched on. */
  proposed: boolean
  next_run_at: string | null
  last_run_at: string | null
  last_run_id: string | null
  last_status: 'started' | 'skipped' | 'error' | null
  last_message: string | null
  created_at: string
  updated_at: string
}

export interface ScheduleInput {
  id: string | null
  project_id: string
  name: string
  prompt: string
  engine: string | null
  permission_mode: string
  timing: Timing
}

/** The editor's fields; every timing kind keeps its own value while switching. */
export interface ScheduleForm {
  id: string | null
  project_id: string
  name: string
  prompt: string
  /** '' = the default engine when it fires. */
  engine: '' | 'claude' | 'codex'
  permission_mode: string
  kind: TimingKind
  every_minutes: number
  time: string
  weekdays: number[]
  once_at: string
}

export type FormError = 'name' | 'prompt' | 'project' | 'every_minutes' | 'time' | 'weekdays' | 'once_at'

const TIME_RE = /^([01]\d|2[0-3]):[0-5]\d$/
const ONCE_RE = /^\d{4}-\d{2}-\d{2}T([01]\d|2[0-3]):[0-5]\d$/

export function emptyForm(projectId: string): ScheduleForm {
  return {
    id: null,
    project_id: projectId,
    name: '',
    prompt: '',
    engine: '',
    permission_mode: 'plan',
    kind: 'daily',
    every_minutes: 60,
    time: '09:00',
    weekdays: [1, 2, 3, 4, 5],
    once_at: '',
  }
}

export function formFromSchedule(s: Schedule): ScheduleForm {
  const form: ScheduleForm = {
    ...emptyForm(s.project_id),
    id: s.id,
    name: s.name,
    prompt: s.prompt,
    engine: s.engine === 'claude' || s.engine === 'codex' ? s.engine : '',
    permission_mode: s.permission_mode,
  }
  const t = s.timing
  if (!t) return form
  form.kind = t.kind
  if (t.kind === 'interval') form.every_minutes = t.every_minutes
  if (t.kind === 'daily' || t.kind === 'weekly') form.time = t.time
  if (t.kind === 'weekly') form.weekdays = [...t.weekdays]
  if (t.kind === 'once') form.once_at = t.at
  return form
}

export function timingFromForm(f: ScheduleForm): Timing {
  switch (f.kind) {
    case 'interval':
      return { kind: 'interval', every_minutes: f.every_minutes }
    case 'daily':
      return { kind: 'daily', time: f.time }
    case 'weekly':
      return { kind: 'weekly', weekdays: [...new Set(f.weekdays)].sort((a, b) => a - b), time: f.time }
    case 'once':
      return { kind: 'once', at: f.once_at }
  }
}

/** The first field the backend would refuse (same rules as `schedules::validate`), or null. */
export function formError(f: ScheduleForm): FormError | null {
  if (!f.project_id) return 'project'
  const name = f.name.trim()
  if (!name || name.length > 200) return 'name'
  if (!f.prompt.trim()) return 'prompt'
  if (f.kind === 'interval') {
    const n = f.every_minutes
    if (!Number.isInteger(n) || n < 1 || n > MAX_INTERVAL_MINUTES) return 'every_minutes'
  }
  if ((f.kind === 'daily' || f.kind === 'weekly') && !TIME_RE.test(f.time)) return 'time'
  if (f.kind === 'weekly' && f.weekdays.length === 0) return 'weekdays'
  if (f.kind === 'once' && !ONCE_RE.test(f.once_at)) return 'once_at'
  return null
}

export function inputFromForm(f: ScheduleForm): ScheduleInput {
  return {
    id: f.id,
    project_id: f.project_id,
    name: f.name.trim(),
    prompt: f.prompt.trim(),
    engine: f.engine || null,
    permission_mode: f.permission_mode,
    timing: timingFromForm(f),
  }
}

/** Toggle a day (0 = Sunday) in a weekday list, kept sorted. */
export function toggleWeekday(days: readonly number[], day: number): number[] {
  return days.includes(day) ? days.filter((d) => d !== day) : [...days, day].sort((a, b) => a - b)
}

/**
 * The i18n key (under `schedules.timing`) and params describing a timing.
 * Weekday params are day numbers; the view turns them into names.
 */
export function timingSummary(t: Timing | null): { key: string; params: Record<string, unknown> } {
  if (!t) return { key: 'invalid', params: {} }
  switch (t.kind) {
    case 'interval':
      return t.every_minutes % 60 === 0
        ? { key: 'everyHours', params: { n: t.every_minutes / 60 } }
        : { key: 'everyMinutes', params: { n: t.every_minutes } }
    case 'daily':
      return { key: 'daily', params: { time: t.time } }
    case 'weekly':
      return { key: 'weekly', params: { time: t.time, days: t.weekdays } }
    case 'once':
      return { key: 'once', params: { at: t.at.replace('T', ' ') } }
  }
}
