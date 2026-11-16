// When a bookmark was placed, for the Bookmarks menu: "10:41 · today", "09:12 ·
// yesterday" or "09:12 · Nov 12". Pure (no Vue / Tauri imports) so it can be
// unit-tested with `node --test`. Local time of the machine, like the History list.

export interface BookmarkWhen {
  /** 24-hour `HH:mm`. */
  time: string
  day: 'today' | 'yesterday' | 'date'
  /** Short date, only when `day` is `'date'`; carries the year when it is not this year. */
  date?: string
}

const two = (n: number) => String(n).padStart(2, '0')
const dayIndex = (d: Date) => Math.floor(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()) / 86_400_000)

/** `null` when `createdAt` is not a date. */
export function bookmarkWhen(createdAt: string, now: Date, locale: string): BookmarkWhen | null {
  const at = new Date(createdAt)
  if (Number.isNaN(at.getTime())) return null
  const time = `${two(at.getHours())}:${two(at.getMinutes())}`
  const ago = dayIndex(now) - dayIndex(at)
  if (ago === 0) return { time, day: 'today' }
  if (ago === 1) return { time, day: 'yesterday' }
  const date = at.toLocaleDateString(locale === 'vi' ? 'vi-VN' : 'en-US', {
    day: 'numeric',
    month: 'short',
    ...(at.getFullYear() === now.getFullYear() ? {} : { year: 'numeric' }),
  })
  return { time, day: 'date', date }
}
