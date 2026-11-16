// Rows of the Running screen's Session tab (V3-53): which live Sessions are
// listed, how a conductor owns its workers, what each is doing and who it waits
// for, plus the filter / sort / stop-target rules. Pure: no Vue, Tauri or `@/`
// imports, so `node --test` can load it.

export type RunningStatus = 'wait' | 'run' | 'done' | 'err' | 'stop'
export type WaitingOn = 'user' | 'agent' | 'worker' | 'none'

/** The slice of a live session this module reads (stores/liveRuns `LiveSession`). */
export interface LiveLike {
  runId: string
  projectId: string
  status: string
  permissionQueue: readonly { tool_name: string; tool_input: unknown }[]
  notifyDone: boolean
  endReason: string | null
  stuck: boolean
  entries: readonly { kind: string; name?: string; displayName?: string; input?: unknown; result?: unknown }[]
}

/** The slice of a run record this module reads (stores/runs `RunRecord`). */
export interface MetaLike {
  project_id: string
  status: string
  engine: string
  model: string | null
  started_at: string | null
  created_at: string
  title: string | null
  run_type: string
  ref_number: number | null
  role?: string | null
  conductor_run_id?: string | null
}

export interface Doing {
  /** `permission`: asking for a tool; `tool`: a call in flight; `workers`: a conductor waiting on workers. */
  kind: 'permission' | 'tool' | 'workers' | 'none'
  name: string
  arg: string
}

export interface RunningRow {
  runId: string
  projectId: string
  projectName: string
  title: string
  engine: string
  model: string
  status: RunningStatus
  /** Permission requests waiting for the user, workers' included for a conductor. */
  pending: number
  isConductor: boolean
  doing: Doing
  startedAt: string | null
  waitingOn: WaitingOn
  /** Running workers of a conductor; they are listed under it, never on their own. */
  workers: RunningRow[]
  /** Finished and not opened yet (the dock's "done" notification). */
  unseen: boolean
  stuck: boolean
  endReason: string | null
}

export type RunningFilter = { status: 'all' | 'wait' | 'run'; projectId: string | null }

const ARG_KEYS = ['command', 'file_path', 'path', 'pattern', 'url', 'query', 'description', 'prompt']

/** One short line for a tool's input: the field a person would recognise, else the first text. */
export function toolArg(input: unknown): string {
  if (typeof input === 'string') return input.trim()
  if (!input || typeof input !== 'object') return ''
  const obj = input as Record<string, unknown>
  for (const k of ARG_KEYS) {
    const v = obj[k]
    if (typeof v === 'string' && v.trim()) return v.trim()
  }
  const first = Object.values(obj).find((v) => typeof v === 'string' && v.trim())
  return typeof first === 'string' ? first.trim() : ''
}

function currentTool(live: LiveLike): Doing {
  const open = [...live.entries].reverse().find((e) => e.kind === 'tool' && e.result === undefined)
  if (!open) return { kind: 'none', name: '', arg: '' }
  return { kind: 'tool', name: open.displayName || open.name || '', arg: toolArg(open.input) }
}

function titleOf(meta: MetaLike | undefined, projectName: string): string {
  if (meta?.title?.trim()) return meta.title.trim()
  if (meta && meta.run_type !== 'session' && meta.ref_number != null) {
    return `${meta.run_type === 'analyze_issue' ? 'Issue' : 'PR'} #${meta.ref_number}`
  }
  return projectName
}

function finishedStatus(status: string): RunningStatus {
  if (status === 'failed') return 'err'
  if (status === 'cancelled') return 'stop'
  return 'done'
}

/**
 * The Session rows. A worker never stands alone: it folds under the conductor
 * that spawned it, and a conductor that finished its own turn while workers run
 * is rebuilt from its run record so the workers do not vanish. A finished worker
 * raises no "just finished" row; only its conductor does.
 */
export function runningRows(
  live: readonly LiveLike[],
  meta: { get(id: string): MetaLike | undefined },
  projects: readonly { id: string; name: string }[],
): RunningRow[] {
  const projectName = (id: string) => projects.find((p) => p.id === id)?.name ?? 'Project'

  const build = (s: LiveLike): RunningRow => {
    const m = meta.get(s.runId)
    const name = projectName(s.projectId)
    const first = s.permissionQueue[0]
    const unseen = s.notifyDone && !m?.conductor_run_id && s.status !== 'running' && s.permissionQueue.length === 0
    const status: RunningStatus =
      s.permissionQueue.length > 0 ? 'wait' : s.status === 'running' ? 'run' : finishedStatus(s.status)
    return {
      runId: s.runId,
      projectId: s.projectId,
      projectName: name,
      title: titleOf(m, name),
      engine: m?.engine ?? '',
      model: m?.model ?? '',
      status,
      pending: s.permissionQueue.length,
      isConductor: m?.role === 'conductor',
      doing: first
        ? { kind: 'permission', name: first.tool_name, arg: toolArg(first.tool_input) }
        : currentTool(s),
      startedAt: m?.started_at ?? m?.created_at ?? null,
      waitingOn: status === 'wait' ? 'user' : status === 'run' ? 'agent' : 'none',
      workers: [],
      unseen,
      stuck: s.stuck,
      endReason: s.endReason,
    }
  }

  const listed = live.filter(
    (s) => s.status === 'running' || s.permissionQueue.length > 0 || (s.notifyDone && !meta.get(s.runId)?.conductor_run_id),
  )
  const byParent = new Map<string, RunningRow[]>()
  const tops: RunningRow[] = []
  const rows = listed.map(build)
  for (const r of rows) {
    const parent = meta.get(r.runId)?.conductor_run_id
    if (!parent) continue
    // A worker that is neither running nor blocked has nothing to show or stop.
    if (r.status !== 'run' && r.status !== 'wait') continue
    byParent.set(parent, [...(byParent.get(parent) ?? []), r])
  }
  for (const r of rows) if (!meta.get(r.runId)?.conductor_run_id) tops.push(r)

  for (const [id, workers] of byParent) {
    let head = tops.find((r) => r.runId === id)
    if (!head) {
      const m = meta.get(id)
      const projectId = m?.project_id ?? workers[0].projectId
      const name = projectName(projectId)
      head = {
        runId: id, projectId, projectName: name, title: titleOf(m, name),
        engine: m?.engine ?? '', model: m?.model ?? '',
        status: 'run', pending: 0, isConductor: true,
        doing: { kind: 'none', name: '', arg: '' },
        startedAt: m?.started_at ?? m?.created_at ?? null, waitingOn: 'agent',
        workers: [], unseen: false, stuck: false, endReason: null,
      }
      tops.push(head)
    }
    head.isConductor = true
    head.workers = workers
    const blocked = workers.reduce((n, w) => n + w.pending, 0)
    head.pending += blocked
    head.stuck = head.stuck || workers.some((w) => w.stuck)
    // The conductor itself is idle while workers run: it waits on them.
    const idle = head.status === 'run' || head.status === 'done' || head.status === 'err' || head.status === 'stop'
    if (head.pending > 0) head.status = 'wait'
    else if (idle) head.status = 'run'
    head.unseen = head.unseen && head.status !== 'run'
    if (head.status === 'wait' && head.waitingOn !== 'user') head.waitingOn = 'user'
    else if (head.status === 'run' && head.doing.kind === 'none') {
      head.doing = { kind: 'workers', name: '', arg: String(workers.length) }
      head.waitingOn = 'worker'
    }
  }
  return tops
}

/** Rows that are still going (running or blocked), as opposed to "just finished". */
export function isActiveRow(r: RunningRow): boolean {
  return r.status === 'run' || r.status === 'wait'
}

function matchesProject(r: RunningRow, projectId: string | null): boolean {
  return projectId === null || r.projectId === projectId
}

/** The active rows the filter lets through (the unseen-finished group has its own list). */
export function filterRunning(rows: readonly RunningRow[], f: RunningFilter): RunningRow[] {
  return rows.filter(
    (r) =>
      isActiveRow(r) &&
      matchesProject(r, f.projectId) &&
      (f.status === 'all' || (f.status === 'wait' ? r.status === 'wait' : r.status === 'run')),
  )
}

/** Finished-but-unseen rows; only the Project filter applies to them. */
export function finishedRunning(rows: readonly RunningRow[], f: RunningFilter): RunningRow[] {
  if (f.status === 'wait') return []
  return rows.filter((r) => !isActiveRow(r) && r.unseen && matchesProject(r, f.projectId))
}

/** "All · Waiting for you · Running" counts, after the Project filter. */
export function runningCounts(rows: readonly RunningRow[], projectId: string | null) {
  const active = rows.filter((r) => isActiveRow(r) && matchesProject(r, projectId))
  const wait = active.filter((r) => r.status === 'wait').length
  return { all: active.length, wait, run: active.length - wait }
}

/** Waiting for you first, then the longest-running (earliest start) first. */
export function sortRunning(rows: readonly RunningRow[]): RunningRow[] {
  const ms = (r: RunningRow) => {
    const t = r.startedAt ? Date.parse(r.startedAt) : NaN
    return Number.isNaN(t) ? Infinity : t
  }
  return [...rows].sort(
    (a, b) => Number(b.status === 'wait') - Number(a.status === 'wait') || ms(a) - ms(b),
  )
}

/** Projects that have at least one listed row, for the Project filter. */
export function runningProjects(rows: readonly RunningRow[]): { id: string; name: string }[] {
  const seen = new Map<string, string>()
  for (const r of rows) if (!seen.has(r.projectId)) seen.set(r.projectId, r.projectName)
  return [...seen].map(([id, name]) => ({ id, name }))
}

/**
 * What "Stop N Sessions" cancels: each selected Session that is still going plus
 * the running workers of the conductors among them. Ids unknown to `rows` drop out.
 */
export function stopTargets(selected: Iterable<string>, rows: readonly RunningRow[]) {
  const want = new Set(selected)
  const sessions: RunningRow[] = []
  const workers: RunningRow[] = []
  for (const r of rows) {
    if (!want.has(r.runId)) continue
    sessions.push(r)
    workers.push(...r.workers)
  }
  return { sessions, workers, ids: [...sessions, ...workers].map((r) => r.runId) }
}
