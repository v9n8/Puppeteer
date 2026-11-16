// Pure helpers of the task detail panel (components/tasks/TaskFieldsGrid.vue):
// one edited cell → the partial patch `update_todo_fields` takes (absent key =
// unchanged, `null` = clear). Every cell saves on its own, so each helper builds
// the patch of that cell only.
import {
  fromLocalInput,
  parseEstimate,
  toLocalInput,
  type AiApply,
  type FocusTodo,
  type TodoPatch,
} from './focus.ts'

/** `ok: false` = the text can't be read (keep the cell open, send nothing); an empty patch = nothing changed. */
export type FieldResult = { ok: true; patch: TodoPatch } | { ok: false }

const unchanged: FieldResult = { ok: true, patch: {} }
const set = (patch: TodoPatch): FieldResult => ({ ok: true, patch })

/** Estimate typed as `30m`, `2h`, `1h30`; empty clears it. */
export function estimatePatch(text: string, todo: Pick<FocusTodo, 'estimate_min'>): FieldResult {
  const min = parseEstimate(text)
  if (min === undefined) return { ok: false }
  return min === todo.estimate_min ? unchanged : set({ estimate_min: min })
}

/** Actual time, same format as the estimate. */
export function actualPatch(text: string, todo: Pick<FocusTodo, 'actual_min'>): FieldResult {
  const min = parseEstimate(text)
  if (min === undefined) return { ok: false }
  return min === todo.actual_min ? unchanged : set({ actual_min: min })
}

/** Deadline date; clearing it clears the time too (the backend does that, so no `due_time` is sent). */
export function dueDatePatch(date: string, todo: Pick<FocusTodo, 'due_date'>): FieldResult {
  const next = date || null
  return next === todo.due_date ? unchanged : set({ due_date: next })
}

/** Deadline time; only meaningful next to a date, so without one nothing is sent. */
export function dueTimePatch(time: string, todo: Pick<FocusTodo, 'due_date' | 'due_time'>): FieldResult {
  if (!todo.due_date) return unchanged
  const next = time || null
  return next === todo.due_time ? unchanged : set({ due_time: next })
}

/** A `datetime-local` value (snooze / reminder): RFC3339 UTC, null when emptied or unreadable. */
export function instantPatch(
  key: 'snoozed_until' | 'remind_at',
  value: string,
  todo: Pick<FocusTodo, 'snoozed_until' | 'remind_at'>,
): FieldResult {
  if (value && !fromLocalInput(value)) return { ok: false }
  const next = fromLocalInput(value)
  // Compare at the minute the input shows, so an unrelated seconds part is not a "change".
  const cur = todo[key] ? fromLocalInput(toLocalInput(todo[key])) : null
  return next === cur ? unchanged : set({ [key]: next })
}

/**
 * "Apply" of an AI analysis: the user pressed it, so it is saved right away
 * (the AI never writes a field on its own, BR-09). An applied estimate is
 * stored as `estimate_source = 'ai'`.
 */
export function aiApplyPatch(a: AiApply): TodoPatch {
  const p: TodoPatch = {}
  if (a.estimateMin != null) {
    p.estimate_min = a.estimateMin
    p.estimate_source = 'ai'
  }
  if (a.priority) p.priority = a.priority
  if (a.dueDate) {
    p.due_date = a.dueDate
    p.due_time = a.dueTime || null
  }
  return p
}
