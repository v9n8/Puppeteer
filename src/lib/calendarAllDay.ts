export interface AllDayItem {
  allDay: boolean
  start: Date
  end: Date
}

/**
 * For each day of a week, the all-day items covering it (input order kept).
 * `end` is the last covered day: parseEventTime already stepped back Google's exclusive end.
 */
export function allDayByDay<T extends AllDayItem>(items: T[], days: Date[]): T[][] {
  return days.map(day =>
    items.filter(
      p => p.allDay && day >= new Date(p.start.getFullYear(), p.start.getMonth(), p.start.getDate()) && day <= p.end,
    ),
  )
}
