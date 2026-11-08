// Editable agent checklist (components/TodoChecklistPicker.vue in the RunView
// composer). The agent plans with Claude's `TodoWrite` tool, shown read-only in
// components/StreamLog.vue. The user edits that list, and the edited list goes
// into the draft as a <session_checklist> block, so the agent gets it with the
// next message. It rides in the message text rather than a hidden prompt: the
// user sees exactly what the agent gets, and no engine needs a new input path.
// Kept free of Vue / Tauri / `@/` imports so it can be unit-tested with `node --test`.

export type TodoStatus = 'pending' | 'in_progress' | 'completed'

export interface TodoItem {
  content: string
  status: TodoStatus
}

const OPEN = '<session_checklist>'
const CLOSE = '</session_checklist>'

// Markdown-checkbox marks: the shape models read best, and one the user can read too.
const MARK: Record<TodoStatus, string> = { pending: ' ', in_progress: '~', completed: 'x' }
const ROW_RE = /^- \[([ ~x])\] (.*)$/

const INSTRUCTIONS =
  'I edited your todo list. Where it differs from the last list you wrote, this one is correct. ' +
  'Update your todo list (TodoWrite) to match it before you go on: keep every item as written, ' +
  'do not re-open an item marked completed, and do not drop an item I added. ' +
  'Legend: [ ] pending, [~] in progress, [x] completed.'
const EMPTY_ROW = '(empty: I removed every item)'

function asStatus(raw: unknown): TodoStatus {
  return raw === 'in_progress' || raw === 'completed' ? raw : 'pending'
}

// One row per line, and no tag that would end the block early when it is read back.
function oneLine(raw: unknown): string {
  return String(raw ?? '')
    .replace(/<\/?session_checklist>/g, '')
    .replace(/\s+/g, ' ')
    .trim()
}

/** Valid items of `raw` (a `TodoWrite` `todos` array, or the editor's rows); blank ones are dropped. */
export function normalizeTodos(raw: unknown): TodoItem[] {
  if (!Array.isArray(raw)) return []
  const out: TodoItem[] = []
  for (const t of raw as unknown[]) {
    const o = t && typeof t === 'object' ? (t as Record<string, unknown>) : {}
    const content = oneLine(o.content)
    if (content) out.push({ content, status: asStatus(o.status) })
  }
  return out
}

/** The block the agent gets: instructions, then one `- [ ] item` row per item. */
export function checklistBlock(todos: readonly TodoItem[]): string {
  const rows = todos.map((t) => `- [${MARK[t.status]}] ${t.content}`)
  return [OPEN, INSTRUCTIONS, '', ...(rows.length ? rows : [EMPTY_ROW]), CLOSE].join('\n')
}

/** The list in the last block of `text`, or null when `text` has no block. */
export function parseChecklistBlock(text: string): TodoItem[] | null {
  const start = text.lastIndexOf(OPEN)
  if (start < 0) return null
  const end = text.indexOf(CLOSE, start)
  if (end < 0) return null
  const items: TodoItem[] = []
  for (const line of text.slice(start + OPEN.length, end).split('\n')) {
    const m = ROW_RE.exec(line.trim())
    if (!m) continue
    const status: TodoStatus = m[1] === 'x' ? 'completed' : m[1] === '~' ? 'in_progress' : 'pending'
    const content = m[2].trim()
    if (content) items.push({ content, status })
  }
  return items
}

/** The parts of a stream entry this file reads (a subset of `StreamEntry`). */
export interface ChecklistEntry {
  kind: string
  name?: string
  input?: unknown
  text?: string
}

/**
 * The newest checklist in a run: the agent's last `TodoWrite`, or a list the
 * user sent after it (the agent may not have rewritten its list yet). Null when
 * the run has neither, so the editor stays hidden.
 */
export function latestChecklist(entries: readonly ChecklistEntry[]): TodoItem[] | null {
  for (let i = entries.length - 1; i >= 0; i--) {
    const e = entries[i]
    if (e.kind === 'tool' && (e.name || '').toLowerCase() === 'todowrite') {
      const input = e.input && typeof e.input === 'object' ? (e.input as Record<string, unknown>) : {}
      return normalizeTodos(input.todos)
    }
    if (e.kind === 'user' && e.text) {
      const sent = parseChecklistBlock(e.text)
      if (sent) return sent
    }
  }
  return null
}

/**
 * `draft` with the block for `todos`: it replaces a block already in the draft
 * (an earlier edit not sent yet), else goes below the typed text after a blank
 * line, leaving a blank line to type under it.
 */
export function mergeChecklistIntoDraft(draft: string, todos: readonly TodoItem[]): string {
  const block = checklistBlock(todos)
  // The last block, the same one parseChecklistBlock reads the editor's list from.
  const start = draft.lastIndexOf(OPEN)
  const end = start < 0 ? -1 : draft.indexOf(CLOSE, start)
  if (end >= 0) return draft.slice(0, start) + block + draft.slice(end + CLOSE.length)
  const head = draft.replace(/\s+$/, '')
  return head ? `${head}\n\n${block}\n\n` : `${block}\n\n`
}

/** Same items in the same order with the same status. */
export function sameTodos(a: readonly TodoItem[], b: readonly TodoItem[]): boolean {
  return a.length === b.length && a.every((t, i) => t.content === b[i].content && t.status === b[i].status)
}

/** The status a click on the status icon moves to: pending → in progress → completed → pending. */
export function nextStatus(status: TodoStatus): TodoStatus {
  return status === 'pending' ? 'in_progress' : status === 'in_progress' ? 'completed' : 'pending'
}
