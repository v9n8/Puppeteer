// Pure helpers for the Calendar screen: the "now" line, the ongoing-event
// state, keyboard shortcuts and the per-account merge used by "Retry".
import { parseEventTime, minutesOfDay } from './calendar.ts'

/** Pixel offset of `now` inside a day column that starts at `dayStartHour`. */
export function nowLineTop(now: Date, dayStartHour: number, hourHeight: number): number {
  return ((minutesOfDay(now) - dayStartHour * 60) / 60) * hourHeight
}

interface TimedLike { start: string; end: string; all_day: boolean }

/** `start <= now < end` for a timed event. All-day events are never "ongoing". */
export function isOngoing(ev: TimedLike, now: Date): boolean {
  if (ev.all_day) return false
  const { start, end } = parseEventTime(ev)
  return start.getTime() <= now.getTime() && now.getTime() < end.getTime()
}

/** Whole minutes left (rounded up) while the event is ongoing, else 0. */
export function minutesLeft(ev: TimedLike, now: Date): number {
  if (!isOngoing(ev, now)) return 0
  return Math.ceil((parseEventTime(ev).end.getTime() - now.getTime()) / 60000)
}

export type CalendarKeyAction = 'prev' | 'next' | 'today' | 'week' | 'month' | 'create'

const KEY_ACTIONS: Record<string, CalendarKeyAction> = {
  ArrowLeft: 'prev',
  ArrowRight: 'next',
  t: 'today',
  w: 'week',
  m: 'month',
  n: 'create',
}

// Anything the user types into, plus dialogs and menus that own the keyboard.
const KEY_BLOCKERS =
  'input,textarea,select,[contenteditable=""],[contenteditable="true"],[role="dialog"],[role="textbox"],[role="combobox"],[role="listbox"],[role="menu"]'

interface KeyEventLike {
  key: string
  metaKey?: boolean
  ctrlKey?: boolean
  altKey?: boolean
  isComposing?: boolean
  defaultPrevented?: boolean
  target?: unknown
}

/** Map a keydown to a Calendar action, or null when the key must be left alone. */
export function calendarKeyAction(e: KeyEventLike): CalendarKeyAction | null {
  if (e.metaKey || e.ctrlKey || e.altKey || e.isComposing || e.defaultPrevented) return null
  const t = e.target as { closest?: (sel: string) => unknown; isContentEditable?: boolean } | null | undefined
  if (t?.isContentEditable || t?.closest?.(KEY_BLOCKERS)) return null
  return KEY_ACTIONS[e.key.length === 1 ? e.key.toLowerCase() : e.key] ?? null
}

/** Replace one account's events with a fresh fetch; other accounts stay untouched. */
export function mergeAccountEvents<E extends { account_id: string }>(
  events: E[],
  accountId: string,
  fresh: E[],
): E[] {
  return [...events.filter(e => e.account_id !== accountId), ...fresh.filter(e => e.account_id === accountId)]
}
