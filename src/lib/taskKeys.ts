// Pure logic of the Tasks screen's keys and of dragging a card to another day
// (V3-39): which key means what, where the selection goes, and what a drop
// writes. The components (components/tasks/useTaskKeyboard.ts, useWeekDrag.ts)
// only wire these to the DOM.
import type { TodoPatch } from './focus'

export type TaskKeyAction = 'next' | 'prev' | 'left' | 'right' | 'done' | 'edit' | 'snooze' | 'open'

/** The part of a keydown that decides; `target` is read by duck typing. */
export interface TaskKeyEvent {
  key: string
  ctrlKey: boolean
  metaKey: boolean
  altKey: boolean
  shiftKey: boolean
  isComposing?: boolean
  keyCode?: number
  target?: unknown
}

interface TargetLike {
  tagName?: string
  isContentEditable?: boolean
  dataset?: Record<string, string | undefined>
}

const TEXT_TAGS = new Set(['INPUT', 'TEXTAREA', 'SELECT'])
const CLICK_TAGS = new Set(['BUTTON', 'A', 'SUMMARY'])

/**
 * True when the key belongs to whatever has focus: a field (the letters are
 * text), a button or link (Space / Enter act on it), unless it is a list row
 * (`data-task-row`) which these keys drive themselves.
 */
function targetKeepsKey(target: unknown): boolean {
  const t = target as TargetLike | null | undefined
  if (!t || typeof t !== 'object') return false
  const tag = t.tagName?.toUpperCase() ?? ''
  if (TEXT_TAGS.has(tag) || t.isContentEditable) return true
  if (CLICK_TAGS.has(tag)) return t.dataset?.taskRow === undefined
  return false
}

/** `week`: ← → move between the day columns (elsewhere they keep their normal job). */
export function taskKeyAction(event: TaskKeyEvent, ctx: { week?: boolean } = {}): TaskKeyAction | null {
  if (event.isComposing || event.keyCode === 229) return null
  if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return null
  if (targetKeepsKey(event.target)) return null
  switch (event.key) {
    case 'ArrowDown':
      return 'next'
    case 'ArrowUp':
      return 'prev'
    case 'ArrowLeft':
      return ctx.week ? 'left' : null
    case 'ArrowRight':
      return ctx.week ? 'right' : null
    case ' ':
    case 'Spacebar':
      return 'done'
    case 'e':
      return 'edit'
    case 's':
      return 'snooze'
    case 'Enter':
      return 'open'
    default:
      return null
  }
}

/** The id one step down / up; no selection (or a vanished one) starts at the first / last. Stops at the ends. */
export function nextSelection(ids: string[], current: string | null, dir: 'next' | 'prev'): string | null {
  if (ids.length === 0) return null
  const at = current === null ? -1 : ids.indexOf(current)
  if (at === -1) return dir === 'next' ? ids[0] : ids[ids.length - 1]
  return ids[Math.min(ids.length - 1, Math.max(0, at + (dir === 'next' ? 1 : -1)))]
}

/**
 * ↑ / ↓: in the week, only inside the column that holds `current` (a day), else over all ids.
 * `columns` is one list per day; no selection (or one in no column) uses the flat list.
 */
export function verticalSelection(
  columns: string[][],
  current: string | null,
  dir: 'next' | 'prev',
  week: boolean,
): string | null {
  const col = week && current !== null ? columns.find((c) => c.includes(current)) : undefined
  return nextSelection(col ?? [...new Set(columns.flat())], current, dir)
}

/**
 * ← / → in the week: the nearest non-empty column on that side, same row
 * (or its last one when shorter). Nothing on that side → stay.
 */
export function weekNeighbour(columns: string[][], current: string | null, dir: 'left' | 'right'): string | null {
  const step = dir === 'right' ? 1 : -1
  const col = columns.findIndex((c) => current !== null && c.includes(current))
  if (col === -1) return columns.find((c) => c.length)?.[0] ?? null
  const row = columns[col].indexOf(current as string)
  for (let i = col + step; i >= 0 && i < columns.length; i += step) {
    if (columns[i].length) return columns[i][Math.min(row, columns[i].length - 1)]
  }
  return current
}

/** What dropping a card on `targetDate` writes: the day changes, the time stays. Null = already there. */
export function moveDueDate(
  due: { due_date: string | null; due_time: string | null },
  targetDate: string,
): TodoPatch | null {
  if (due.due_date === targetDate) return null
  return { due_date: targetDate, due_time: due.due_time }
}
