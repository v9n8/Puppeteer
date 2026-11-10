// Titlebar project switcher (UX-09): the recently-used order, the filtered list
// the popover shows and the ⌘` "previous project". Kept free of Vue / Tauri /
// `@/` imports so it can be unit-tested with `node --test`.

/** How many recently used projects the store remembers. */
export const MRU_LIMIT = 10

/** The fields of a project the switcher reads. */
export interface SwitcherProject {
  id: string
  name: string
  path: string
  hidden: boolean
}

export interface SwitcherRow<T extends SwitcherProject> {
  project: T
  /** Among the recently used projects (listed first). */
  recent: boolean
  /** Another shown project has the same name, so the row shows its path too. */
  showPath: boolean
}

/** `mru` with `projectId` moved to the front, each id once, at most `limit` long. */
export function touchMru(mru: readonly string[], projectId: string, limit = MRU_LIMIT): string[] {
  if (!projectId) return [...mru]
  return [projectId, ...mru.filter((id) => id !== projectId)].slice(0, limit)
}

/**
 * The project ⌘` goes back to: the most recent one other than `currentId` that
 * still exists. Off a project screen (`currentId` null) that is the last project
 * used, so the shortcut also returns from Settings or Skills.
 */
export function previousProject(
  mru: readonly string[],
  currentId: string | null,
  existing: readonly { id: string }[],
): string | null {
  const ids = new Set(existing.map((p) => p.id))
  return mru.find((id) => id !== currentId && ids.has(id)) ?? null
}

/**
 * The rows of the popover: the projects whose name or path contains `query`
 * (any case), the recently used ones first in MRU order, then the rest in the
 * store's order (the user's manual order). Hidden projects only with
 * `showHidden`. Enter opens the first row.
 */
export function switcherRows<T extends SwitcherProject>(
  projects: readonly T[],
  opts: { query: string; mru: readonly string[]; showHidden: boolean },
): SwitcherRow<T>[] {
  const q = opts.query.trim().toLowerCase()
  const shown = projects.filter(
    (p) =>
      (opts.showHidden || !p.hidden) &&
      (!q || p.name.toLowerCase().includes(q) || p.path.toLowerCase().includes(q)),
  )
  const rank = new Map(opts.mru.map((id, i) => [id, i]))
  const recent = shown
    .filter((p) => rank.has(p.id))
    .sort((a, b) => (rank.get(a.id) as number) - (rank.get(b.id) as number))
  const rest = shown.filter((p) => !rank.has(p.id))
  const nameCount = new Map<string, number>()
  for (const p of shown) {
    const key = p.name.trim().toLowerCase()
    nameCount.set(key, (nameCount.get(key) ?? 0) + 1)
  }
  const row = (project: T, isRecent: boolean): SwitcherRow<T> => ({
    project,
    recent: isRecent,
    showPath: (nameCount.get(project.name.trim().toLowerCase()) ?? 0) > 1,
  })
  return [...recent.map((p) => row(p, true)), ...rest.map((p) => row(p, false))]
}

/** The row the arrow keys move to from `index`, wrapping at both ends; -1 when there are none. */
export function stepIndex(index: number, delta: 1 | -1, count: number): number {
  if (count <= 0) return -1
  if (index < 0) return delta > 0 ? 0 : count - 1
  return (index + delta + count) % count
}

/**
 * Whether `add_project` brought a project back from the Trash rather than
 * creating it: a new row is stamped "now" and has no sessions, a restored one
 * keeps its sessions and its original creation time. `startedAt` is when the
 * add began (ms); a minute of slack absorbs clock skew with a remote desktop.
 */
export function wasRestored(project: { created_at: string; run_count: number }, startedAt: number): boolean {
  if (project.run_count > 0) return true
  const created = Date.parse(project.created_at)
  return Number.isFinite(created) && created < startedAt - 60_000
}
