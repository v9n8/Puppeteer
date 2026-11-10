// Folding a turn's work into one block: the tool calls, thinking and the
// commentary between them collapse under a "N steps · M files edited" header
// (components/sessions/TurnActivityHeader.vue, placed by components/StreamLog.vue),
// while the prompt and the final answer stay in view. Kept free of Vue / DOM /
// `@/` imports so it can be unit-tested with `node --test`.
import { toolFileTargets } from './toolFiles.ts'

interface EntryLike {
  kind: string
  text?: string
  name?: string
  input?: unknown
  source?: string
  label?: string
  result?: { content?: string; is_error?: boolean }
  duration_ms?: number
}

/** One turn's folded work. */
export interface TurnActivity {
  /** Entry index the header sits above: the block's first folded entry. */
  at: number
  /**
   * Entry whose identity keys the block's open / closed choice: the turn's
   * prompt, or `at` when the prompt is before the loaded window. An entry, not
   * an index, so the choice survives older history being prepended.
   */
  keyAt: number
  /**
   * Folded entries, ascending. Entries between them that are not listed (a
   * plan, a question to the user, an error) stay visible even when folded.
   */
  members: number[]
  /** Tool calls and thinking blocks. */
  steps: number
  /** Distinct files the turn's successful edit / write calls touched. */
  filesEdited: number
  /** Tool calls that came back with an error. */
  errors: number
  /** The turn's wall time, from its result entry; null while running or unknown. */
  durationMs: number | null
  /** The turn is still being written. */
  running: boolean
}

export interface TurnActivityMap {
  blocks: TurnActivity[]
  /** Folded entry index → its block (index into `blocks`). */
  memberOf: Map<number, number>
  /** Entry index → the block whose header sits right above it. */
  headerAt: Map<number, number>
}

const lower = (s: string | undefined) => (s || '').toLowerCase()

// Tools the user has to read or answer: a question and a proposed plan stay in
// view when their turn is folded.
const SHOWN_TOOLS = new Set(['askuserquestion', 'exitplanmode'])
const EDIT_TOOLS = new Set(['edit', 'multiedit', 'write', 'notebookedit'])

/** A step of the turn's work: a tool call or a thinking block. */
function isStep(e: EntryLike): boolean {
  return e.kind === 'tool' || e.kind === 'thinking'
}

/**
 * Whether an entry inside the work part of a turn folds away. Prose, tool
 * calls, thinking, diagnostic lines and skill / meta injections do. A prompt,
 * the turn's result, stderr, a compaction divider, a slash command, IDE
 * context, a conductor nudge and the tools in SHOWN_TOOLS never do.
 */
export function foldsAway(e: EntryLike): boolean {
  switch (e.kind) {
    case 'tool':
      return !SHOWN_TOOLS.has(lower(e.name))
    case 'thinking':
    case 'text':
    case 'log':
      return true
    case 'injected':
      return e.source !== 'session'
    default:
      return false
  }
}

/**
 * The fold blocks of a log. A turn runs from a `user` entry to just before the
 * next one, as in lib/turnActions `turnFooters`; entries before the first
 * prompt are the tail of a turn whose prompt is outside the loaded window.
 * Its work part ends at its last step: assistant text after that is the answer
 * and stays out of the block, text before it is commentary and folds in. A
 * turn with no foldable step gets no block. While `running`, the last turn is
 * the running one; text after its latest step counts as the answer until a
 * new step follows it.
 */
export function turnActivity(entries: readonly EntryLike[], running: boolean): TurnActivityMap {
  const blocks: TurnActivity[] = []
  const memberOf = new Map<number, number>()
  const headerAt = new Map<number, number>()

  const closeTurn = (promptAt: number, end: number) => {
    const from = promptAt + 1
    let lastStep = -1
    for (let i = end - 1; i >= from; i--) {
      if (isStep(entries[i])) {
        lastStep = i
        break
      }
    }
    if (lastStep === -1) return
    const members: number[] = []
    let steps = 0
    let errors = 0
    const files = new Set<string>()
    for (let i = from; i <= lastStep; i++) {
      const e = entries[i]
      if (!foldsAway(e)) continue
      members.push(i)
      if (!isStep(e)) continue
      steps++
      if (e.kind !== 'tool') continue
      if (e.result?.is_error) errors++
      else if (EDIT_TOOLS.has(lower(e.name))) for (const f of toolFileTargets(e.input)) files.add(f)
    }
    if (!steps) return
    const isRunning = running && end === entries.length
    let durationMs: number | null = null
    if (!isRunning) {
      for (let i = end - 1; i > lastStep; i--) {
        const e = entries[i]
        if (e.kind === 'result') {
          durationMs = typeof e.duration_ms === 'number' && e.duration_ms > 0 ? e.duration_ms : null
          break
        }
      }
    }
    const b = blocks.length
    blocks.push({
      at: members[0],
      keyAt: promptAt >= 0 ? promptAt : members[0],
      members,
      steps,
      filesEdited: files.size,
      errors,
      durationMs,
      running: isRunning,
    })
    headerAt.set(members[0], b)
    for (const i of members) memberOf.set(i, b)
  }

  let promptAt = -1
  for (let i = 0; i < entries.length; i++) {
    if (entries[i].kind !== 'user') continue
    closeTurn(promptAt, i)
    promptAt = i
  }
  closeTurn(promptAt, entries.length)
  return { blocks, memberOf, headerAt }
}

/**
 * Whether a block shows its entries. A choice the reader made on this block
 * wins; otherwise a ⌘F hit inside it, the running turn, or the viewer's
 * "expand all" opens it.
 */
export function activityOpen(
  block: Pick<TurnActivity, 'running'>,
  opts: { choice?: boolean; findHit?: boolean; expandAll?: boolean },
): boolean {
  if (typeof opts.choice === 'boolean') return opts.choice
  return !!opts.findHit || block.running || !!opts.expandAll
}

// The text a folded entry shows once its block is open (tool cards stay
// collapsed, so only their header line): prose, the tool's name and the short
// string fields its header is built from.
const HEADER_INPUT_KEYS = [
  'description', 'command', 'cmd', 'file_path', 'notebook_path', 'path', 'pattern', 'query', 'url',
  'subagent_type', 'message', 'toolName', 'serverName',
]

function visibleText(e: EntryLike): string {
  if (e.kind === 'tool') {
    const parts = [e.name ?? '']
    const input = e.input
    if (typeof input === 'string') parts.push(input)
    else if (input && typeof input === 'object') {
      const o = input as Record<string, unknown>
      for (const k of HEADER_INPUT_KEYS) if (typeof o[k] === 'string') parts.push(o[k] as string)
      parts.push(...toolFileTargets(input))
    }
    return parts.join('\n')
  }
  if (e.kind === 'injected') return e.label ?? ''
  return e.text ?? ''
}

/**
 * Whether a ⌘F query appears in what a block shows when open, so the block can
 * open itself for the in-page highlight to land on. Errs on matching: opening
 * a block with nothing highlighted costs less than hiding a match.
 */
export function activityMatches(
  entries: readonly EntryLike[],
  block: Pick<TurnActivity, 'members'>,
  query: string,
  caseSensitive = false,
): boolean {
  if (!query) return false
  const q = caseSensitive ? query : query.toLowerCase()
  return block.members.some((i) => {
    const e = entries[i]
    if (!e) return false
    const text = visibleText(e)
    return (caseSensitive ? text : text.toLowerCase()).includes(q)
  })
}

/** Header duration: `850ms`, `12.3s`, `4m 05s`, `1h 02m`. */
export function formatActivityDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`
  const totalSec = Math.round(ms / 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  if (totalSec < 3600) return `${Math.floor(totalSec / 60)}m ${pad(totalSec % 60)}s`
  const totalMin = Math.floor(totalSec / 60)
  return `${Math.floor(totalMin / 60)}h ${pad(totalMin % 60)}m`
}
