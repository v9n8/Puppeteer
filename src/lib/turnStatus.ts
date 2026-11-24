// What the session's last turn is doing, for the turn status bar above the composer
// (components/sessions/TurnStatusBar.vue): one status for the bar, the turn's
// steps and its todo list for the list that opens from it. Pure functions of the
// entries and the live state, free of Vue / DOM / `@/` imports so they can be
// unit-tested with `node --test`.
import { normalizeTodos, type TodoItem } from './todoChecklist.ts'
import { taskLabel, type BackgroundState } from './backgroundTasks.ts'

export interface TurnEntryLike {
  kind: string
  text?: string
  name?: string
  displayName?: string
  input?: unknown
  turn?: number
  result?: { content?: string; is_error?: boolean }
  elapsedSecs?: number
  startedAt?: number
  endedAt?: number
  duration_ms?: number
  total_tokens?: number
  is_error?: boolean
}

export type TurnStatus =
  | { kind: 'none' }
  | { kind: 'loading' }
  | { kind: 'done'; turn: number; durationMs: number | null; tokens: number | null }
  | { kind: 'background'; turn: number; durationMs: number | null; tokens: number | null; tasks: string[] }
  | { kind: 'tool'; turn: number; tool: string; detail: string; startedAt: number | null; elapsedSecs: number | null }
  | { kind: 'thinking'; turn: number; retry: { attempt: number; maxRetries: number; delaySecs: number } | null }
  | { kind: 'permission'; turn: number; detail: string }
  | { kind: 'question'; turn: number; text: string }
  | { kind: 'error'; turn: number; text: string }
  | { kind: 'cancelled'; turn: number }

export interface TurnStatusInput {
  entries: readonly TurnEntryLike[]
  /** The run is live: a turn is being written. */
  running: boolean
  /** The log is still being read from disk. */
  loading?: boolean
  /** The run's status: `done`, `failed`, `cancelled`, `fetched`… */
  runStatus?: string
  liveStatus?: { kind: string; attempt?: number; maxRetries?: number; delayMs?: number } | null
  /** The head permission request / question of the run. */
  pending?: { toolName: string; toolInput: unknown } | null
  /** `RunRecord.end_reason`: why the run stopped when the user did not stop it. */
  endReason?: string | null
  /** The live session's turn / background tasks (`lib/backgroundTasks`). */
  background?: BackgroundState | null
}

/** The turn a status line or step list is about: its prompt's position and its entries. */
export interface LastTurn {
  /** 1-based number; the prompt's own `turn` when the log has it. */
  turn: number
  /** Index of the turn's first entry after its prompt. */
  from: number
}

const lower = (s: string | undefined) => (s || '').toLowerCase()

/** The last turn of a log, or null when it has no prompt. */
export function lastTurn(entries: readonly TurnEntryLike[]): LastTurn | null {
  let count = 0
  let at = -1
  for (let i = 0; i < entries.length; i++) {
    if (entries[i].kind !== 'user') continue
    count++
    at = i
  }
  if (at < 0) return null
  const own = entries[at].turn
  return { turn: typeof own === 'number' && own > 0 ? own : count, from: at + 1 }
}

// The input fields a one-line summary of a tool call is taken from, in order.
const DETAIL_KEYS = ['command', 'cmd', 'file_path', 'notebook_path', 'path', 'pattern', 'query', 'url', 'description', 'prompt']

/** A tool call's input on one line: its command, file, pattern or query. */
export function toolDetail(input: unknown, max = 160): string {
  let text = ''
  if (typeof input === 'string') text = input
  else if (input && typeof input === 'object') {
    const o = input as Record<string, unknown>
    for (const k of DETAIL_KEYS) {
      if (typeof o[k] === 'string' && o[k]) {
        text = o[k] as string
        break
      }
    }
  }
  text = text.replace(/\s+/g, ' ').trim()
  return text.length > max ? `${text.slice(0, max - 1)}…` : text
}

export type StepKind = 'read' | 'edit' | 'write' | 'bash' | 'search' | 'web' | 'agent' | 'todo' | 'ask' | 'other'

/** The group a tool belongs to (its label in the step list). */
export function stepKind(name: string | undefined): StepKind {
  const n = lower(name)
  if (n === 'read') return 'read'
  if (n === 'edit' || n === 'multiedit' || n === 'notebookedit') return 'edit'
  if (n === 'write') return 'write'
  if (n === 'bash' || n === 'bashoutput' || n === 'killshell' || n === 'killbash') return 'bash'
  if (n === 'grep' || n === 'glob' || n === 'toolsearch') return 'search'
  if (n === 'webfetch' || n === 'websearch') return 'web'
  if (n === 'task' || n === 'agent') return 'agent'
  if (n === 'todowrite') return 'todo'
  if (n === 'askuserquestion') return 'ask'
  return 'other'
}

export interface TurnStep {
  /** Entry index in the log. */
  at: number
  kind: StepKind
  /** The tool's name as the engine calls it (shown for kind `other`). */
  name: string
  detail: string
  state: 'running' | 'done' | 'error'
  /** From the log's step marks, else the engine's own progress; null when unknown. */
  durationMs: number | null
  /** Epoch ms the call started, while it runs; the list ticks its time from it. */
  startedAt: number | null
}

function stepDuration(e: TurnEntryLike): number | null {
  if (typeof e.startedAt === 'number' && typeof e.endedAt === 'number' && e.endedAt >= e.startedAt) {
    return e.endedAt - e.startedAt
  }
  return typeof e.elapsedSecs === 'number' ? e.elapsedSecs * 1000 : null
}

/** The tool calls of the last turn, in order. */
export function turnSteps(entries: readonly TurnEntryLike[]): TurnStep[] {
  const turn = lastTurn(entries)
  if (!turn) return []
  const out: TurnStep[] = []
  for (let i = turn.from; i < entries.length; i++) {
    const e = entries[i]
    if (e.kind !== 'tool') continue
    const state = !e.result ? 'running' : e.result.is_error ? 'error' : 'done'
    out.push({
      at: i,
      kind: stepKind(e.name),
      name: e.displayName || e.name || 'tool',
      detail: toolDetail(e.input),
      state,
      durationMs: state === 'running' ? null : stepDuration(e),
      startedAt: state === 'running' && typeof e.startedAt === 'number' ? e.startedAt : null,
    })
  }
  return out
}

/** The agent's last `TodoWrite` list of the last turn; empty when the turn wrote none. */
export function turnTodos(entries: readonly TurnEntryLike[]): TodoItem[] {
  const turn = lastTurn(entries)
  if (!turn) return []
  for (let i = entries.length - 1; i >= turn.from; i--) {
    const e = entries[i]
    if (e.kind !== 'tool' || lower(e.name) !== 'todowrite') continue
    const input = e.input && typeof e.input === 'object' ? (e.input as Record<string, unknown>) : {}
    return normalizeTodos(input.todos)
  }
  return []
}

/** Finished / total of a todo list. */
export function todoProgress(todos: readonly TodoItem[]): { done: number; total: number } {
  return { done: todos.filter((t) => t.status === 'completed').length, total: todos.length }
}

function questionText(input: unknown): string {
  const o = input && typeof input === 'object' ? (input as Record<string, unknown>) : {}
  const qs = Array.isArray(o.questions) ? (o.questions as unknown[]) : []
  const first = qs[0] && typeof qs[0] === 'object' ? (qs[0] as Record<string, unknown>) : {}
  return typeof first.question === 'string' ? first.question.replace(/\s+/g, ' ').trim() : ''
}

const PAUSED_REASONS = new Set(['app_closed', 'quota'])

function lastResult(entries: readonly TurnEntryLike[], from: number): TurnEntryLike | undefined {
  for (let i = entries.length - 1; i >= from; i--) {
    if (entries[i].kind === 'result') return entries[i]
  }
  return undefined
}

const positive = (n: number | undefined) => (typeof n === 'number' && n > 0 ? n : null)

/** The status of the session's last turn. */
export function turnStatus(input: TurnStatusInput): TurnStatus {
  if (input.loading) return { kind: 'loading' }
  const last = lastTurn(input.entries)
  if (!last) return { kind: 'none' }
  const { entries } = input
  const turn = last.turn

  if (input.running) {
    const p = input.pending
    if (p) {
      if (lower(p.toolName) === 'askuserquestion') return { kind: 'question', turn, text: questionText(p.toolInput) }
      return { kind: 'permission', turn, detail: toolDetail(p.toolInput) || p.toolName }
    }
    // The turn is over, only its background tasks keep the session up. The
    // backend's word plus the turn's own `result`, so a snapshot that lands
    // before the result's entry does not flash "done" over a running turn.
    const bg = input.background
    const ended = bg?.turnDone && bg.tasks.length ? lastResult(entries, last.from) : undefined
    if (bg && ended) {
      return {
        kind: 'background',
        turn,
        durationMs: positive(ended.duration_ms),
        tokens: positive(ended.total_tokens),
        tasks: bg.tasks.map(taskLabel),
      }
    }
    // A tool call with no result yet, and nothing after it: the turn is inside it.
    for (let i = entries.length - 1; i >= last.from; i--) {
      const e = entries[i]
      if (e.kind === 'tool' && !e.result) {
        return {
          kind: 'tool',
          turn,
          tool: e.displayName || e.name || 'tool',
          detail: toolDetail(e.input),
          startedAt: typeof e.startedAt === 'number' ? e.startedAt : null,
          elapsedSecs: typeof e.elapsedSecs === 'number' ? e.elapsedSecs : null,
        }
      }
      if (e.kind === 'text' || e.kind === 'thinking' || e.kind === 'tool') break
    }
    const ls = input.liveStatus
    const retry = ls?.kind === 'retry'
      ? {
          attempt: ls.attempt ?? 0,
          maxRetries: ls.maxRetries ?? 0,
          delaySecs: Math.max(0, Math.round((ls.delayMs ?? 0) / 1000)),
        }
      : null
    return { kind: 'thinking', turn, retry }
  }

  if (input.runStatus === 'cancelled' || (input.endReason && PAUSED_REASONS.has(input.endReason))) {
    return { kind: 'cancelled', turn }
  }
  const result = lastResult(entries, last.from)
  if (result?.is_error) return { kind: 'error', turn, text: (result.text || '').replace(/\s+/g, ' ').trim() }
  if (!result) {
    for (let i = entries.length - 1; i >= last.from; i--) {
      if (entries[i].kind === 'error') return { kind: 'error', turn, text: (entries[i].text || '').replace(/\s+/g, ' ').trim() }
    }
    if (input.runStatus === 'failed') return { kind: 'error', turn, text: '' }
  }
  return { kind: 'done', turn, durationMs: positive(result?.duration_ms), tokens: positive(result?.total_tokens) }
}
