// Tabs of the Session area (V3-24): "Conversation" first, then the files the
// session opened. Each Session (run id) has its own set, so switching Session
// switches the tab bar; it is kept per machine in localStorage
// (stores/sessionTabs). Pure functions only, so the rules are testable.
//
// Every function returns a new state; the input is never mutated.

export type SessionTabId = 'conversation' | 'changed' | `file:${string}`

export interface FileTab {
  id: `file:${string}`
  kind: 'file'
  /** As openFile got it: project-relative, or absolute when outside the Project. */
  path: string
  /** Line to jump to, from the latest open. */
  line: number | null
  pinned: boolean
}

/** The "Changed files" tab (V3-26). */
export interface ChangedFilesTab {
  id: 'changed'
  kind: 'changed'
  pinned: boolean
}

export type SessionTab = FileTab | ChangedFilesTab

export interface SessionTabsState {
  /** Without 'conversation': it always comes first and is never closed or pinned. */
  tabs: SessionTab[]
  active: SessionTabId
  /** Split view (V3-27); only reserved here. */
  split?: { on: boolean; ratio: number; rightActive: SessionTabId | null }
  /** For pruning: the Sessions used least recently go first. */
  usedAt: number
}

export const CONVERSATION: SessionTabId = 'conversation'

/** Sessions remembered at most; older ones are dropped. */
export const MAX_SESSIONS = 100

export function emptyState(now = 0): SessionTabsState {
  return { tabs: [], active: CONVERSATION, usedAt: now }
}

export function fileTabId(path: string): `file:${string}` {
  return `file:${path}`
}

/** Pinned tabs right after Conversation, then the rest in the order they were opened. */
export function orderedTabs(state: SessionTabsState): SessionTab[] {
  return [...state.tabs.filter((t) => t.pinned), ...state.tabs.filter((t) => !t.pinned)]
}

function hasTab(state: SessionTabsState, id: SessionTabId): boolean {
  return id === CONVERSATION || state.tabs.some((t) => t.id === id)
}

/** Open `path` as a tab, or select its tab when it is already open (moving to `line` when given). */
export function openTab(state: SessionTabsState, path: string, line: number | null, now = state.usedAt): SessionTabsState {
  const id = fileTabId(path)
  const existing = state.tabs.find((t) => t.id === id)
  const tabs: SessionTab[] = existing
    ? state.tabs.map((t) => (t.id === id && t.kind === 'file' ? { ...t, line: line ?? t.line } : t))
    : [...state.tabs, { id, kind: 'file', path, line, pinned: false }]
  return { ...state, tabs, active: id, usedAt: now }
}

export function selectTab(state: SessionTabsState, id: SessionTabId, now = state.usedAt): SessionTabsState {
  if (!hasTab(state, id)) return state
  return { ...state, active: id, usedAt: now }
}

/** Close a tab. Closing the selected one selects the tab on its left (Conversation last). */
export function closeTab(state: SessionTabsState, id: SessionTabId): SessionTabsState {
  if (id === CONVERSATION || !state.tabs.some((t) => t.id === id)) return state
  let active = state.active
  if (active === id) {
    const order = orderedTabs(state)
    const i = order.findIndex((t) => t.id === id)
    active = i > 0 ? order[i - 1].id : CONVERSATION
  }
  return { ...state, tabs: state.tabs.filter((t) => t.id !== id), active }
}

/** Keep Conversation, the pinned tabs and `id`. */
export function closeOthers(state: SessionTabsState, id: SessionTabId): SessionTabsState {
  const tabs = state.tabs.filter((t) => t.pinned || t.id === id)
  const active = tabs.some((t) => t.id === state.active) || state.active === CONVERSATION
    ? state.active
    : hasTab({ ...state, tabs }, id) ? id : CONVERSATION
  return { ...state, tabs, active }
}

export function togglePin(state: SessionTabsState, id: SessionTabId): SessionTabsState {
  if (!state.tabs.some((t) => t.id === id)) return state
  const flipped = { ...state, tabs: state.tabs.map((t) => (t.id === id ? { ...t, pinned: !t.pinned } : t)) }
  // Keep the stored order the shown order, so the tab stays where it landed.
  return { ...flipped, tabs: orderedTabs(flipped) }
}

/** Follow a renamed file (or a file under a renamed folder). */
export function renameTab(state: SessionTabsState, oldPath: string, newPath: string): SessionTabsState {
  const moved = (p: string) =>
    p === oldPath ? newPath : p.startsWith(`${oldPath}/`) ? newPath + p.slice(oldPath.length) : null
  let active = state.active
  const tabs: SessionTab[] = []
  for (const t of state.tabs) {
    const next = t.kind === 'file' ? moved(t.path) : null
    if (t.kind !== 'file' || next === null) {
      tabs.push(t)
      continue
    }
    const id = fileTabId(next)
    if (active === t.id) active = id
    // The new path already had a tab: keep that one.
    if (state.tabs.some((o) => o.id === id)) continue
    tabs.push({ ...t, id, path: next })
  }
  return { ...state, tabs, active }
}

/**
 * Drop Sessions that are gone (only when `liveRunIds` is known), Sessions with
 * nothing to remember, and keep the MAX_SESSIONS most recently used. A usedAt in
 * the future (clock moved back) counts as `now`, so it cannot stay forever.
 */
export function prune(
  all: Record<string, SessionTabsState>,
  liveRunIds: ReadonlySet<string> | null,
  now: number,
): Record<string, SessionTabsState> {
  const kept = Object.entries(all)
    .filter(([runId, s]) => (!liveRunIds || liveRunIds.has(runId)) && (s.tabs.length > 0 || s.split?.on))
    .map(([runId, s]) => [runId, s.usedAt > now ? { ...s, usedAt: now } : s] as const)
    .sort((a, b) => b[1].usedAt - a[1].usedAt)
    .slice(0, MAX_SESSIONS)
  return Object.fromEntries(kept)
}

export function serialize(all: Record<string, SessionTabsState>): string {
  return JSON.stringify(all)
}

function isObject(v: unknown): v is Record<string, unknown> {
  return !!v && typeof v === 'object' && !Array.isArray(v)
}

function parseTab(raw: unknown): SessionTab | null {
  if (!isObject(raw)) return null
  const pinned = raw.pinned === true
  if (raw.kind === 'changed') return { id: 'changed', kind: 'changed', pinned }
  if (raw.kind !== 'file' || typeof raw.path !== 'string' || !raw.path) return null
  const line = typeof raw.line === 'number' && Number.isInteger(raw.line) && raw.line > 0 ? raw.line : null
  return { id: fileTabId(raw.path), kind: 'file', path: raw.path, line, pinned }
}

function parseState(raw: unknown): SessionTabsState | null {
  if (!isObject(raw) || !Array.isArray(raw.tabs)) return null
  const tabs: SessionTab[] = []
  for (const r of raw.tabs) {
    const tab = parseTab(r)
    if (tab && !tabs.some((t) => t.id === tab.id)) tabs.push(tab)
  }
  const state: SessionTabsState = {
    tabs,
    active: CONVERSATION,
    usedAt: typeof raw.usedAt === 'number' && Number.isFinite(raw.usedAt) ? raw.usedAt : 0,
  }
  const split = raw.split
  if (isObject(split) && typeof split.on === 'boolean' && typeof split.ratio === 'number') {
    const right = typeof split.rightActive === 'string' && hasTab(state, split.rightActive as SessionTabId)
      ? (split.rightActive as SessionTabId)
      : null
    state.split = { on: split.on, ratio: split.ratio, rightActive: right }
  }
  if (typeof raw.active === 'string' && hasTab(state, raw.active as SessionTabId)) {
    state.active = raw.active as SessionTabId
  }
  return state
}

/** Read what serialize wrote. Records of the wrong shape are dropped; broken JSON is empty. */
export function parse(raw: string | null): Record<string, SessionTabsState> {
  if (!raw) return {}
  let obj: unknown
  try {
    obj = JSON.parse(raw)
  } catch {
    return {}
  }
  if (!isObject(obj)) return {}
  const out: Record<string, SessionTabsState> = {}
  for (const [runId, v] of Object.entries(obj)) {
    const state = parseState(v)
    if (runId && state) out[runId] = state
  }
  return out
}

/** File name, and the folder it sits in (empty at the root), for a tab label. */
export function tabLabel(path: string): { name: string; dir: string } {
  const i = path.lastIndexOf('/')
  return i === -1 ? { name: path, dir: '' } : { name: path.slice(i + 1) || path, dir: path.slice(0, i) }
}

export type FileTabIcon = 'code' | 'json' | 'image' | 'text' | 'file'

const CODE_EXT = new Set([
  'ts', 'tsx', 'js', 'jsx', 'mjs', 'cjs', 'vue', 'svelte', 'rs', 'py', 'go', 'rb', 'java', 'kt', 'swift', 'c', 'h',
  'cc', 'cpp', 'hpp', 'cs', 'php', 'sh', 'zsh', 'bash', 'sql', 'css', 'scss', 'html', 'xml', 'toml', 'yaml', 'yml',
])
const IMAGE_EXT = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico', 'avif'])
const TEXT_EXT = new Set(['md', 'markdown', 'txt', 'log', 'csv', 'rst', 'adoc'])

/** Which icon a file tab shows, from its extension. */
export function fileTabIcon(path: string): FileTabIcon {
  const name = tabLabel(path).name.toLowerCase()
  const dot = name.lastIndexOf('.')
  const ext = dot > 0 ? name.slice(dot + 1) : ''
  if (ext === 'json' || ext === 'jsonl') return 'json'
  if (CODE_EXT.has(ext)) return 'code'
  if (IMAGE_EXT.has(ext)) return 'image'
  if (TEXT_EXT.has(ext)) return 'text'
  return 'file'
}

/**
 * Tabs that are not fully inside the scroll viewport [scrollLeft, scrollLeft + width].
 * `boxes` are the tabs' left offsets and widths within the scrolled content.
 */
export function hiddenTabIndexes(boxes: { left: number; width: number }[], scrollLeft: number, width: number): number[] {
  const out: number[] = []
  boxes.forEach((b, i) => {
    // 1px slack: sub-pixel layout rounds offsets either way.
    if (b.left < scrollLeft - 1 || b.left + b.width > scrollLeft + width + 1) out.push(i)
  })
  return out
}
