// Project list helpers (V3-45): the live-session dot of a project row, and the
// middle-cut path that keeps the folder name visible.

export type RunDot = 'waiting' | 'running'

export interface RunDotSession {
  projectId: string
  status: string
  permissionQueue: { length: number }
}

/** One dot per project with a live session. A waiting permission beats running. */
export function projectRunDots(sessions: Iterable<RunDotSession>): Record<string, RunDot> {
  const dots: Record<string, RunDot> = {}
  for (const s of sessions) {
    if (s.permissionQueue.length > 0) dots[s.projectId] = 'waiting'
    else if (s.status === 'running' && dots[s.projectId] !== 'waiting') dots[s.projectId] = 'running'
  }
  return dots
}

export interface MiddlePath {
  head: string
  middle: string
  tail: string
}

/**
 * Split a path so CSS can cut only `middle`: `head` is the first folders,
 * `tail` the last one. A path within `budget` characters, or too short to have
 * a middle, comes back whole in `head`.
 */
export function middlePath(path: string, budget: number): MiddlePath {
  const whole = { head: path, middle: '', tail: '' }
  if (path.length <= budget) return whole
  const sep = path.includes('/') ? '/' : '\\'
  const parts = path.split(sep)
  // "/Users/me" starts with an empty part: keep one more.
  const keep = parts[0] === '' ? 3 : 2
  if (parts.length <= keep + 1) return whole
  return {
    head: parts.slice(0, keep).join(sep),
    middle: sep + parts.slice(keep, -1).join(sep),
    tail: sep + parts[parts.length - 1],
  }
}
