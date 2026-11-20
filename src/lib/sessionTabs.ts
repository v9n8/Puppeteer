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

export interface SplitState {
  on: boolean
  ratio: number
  rightActive: SessionTabId | null
}

export interface SessionTabsState {
  /** Without 'conversation': it always comes first and is never closed or pinned. */
  tabs: SessionTab[]
  active: SessionTabId
  /**
   * Split view (V3-27): Conversation on the left, the file tabs on the right.
   * `ratio` is the left share (0..1). While `on`, `active` keeps following the
   * file tab picked on the right, so dropping back to the tab layout (a narrow
   * window, "Unsplit") lands on the same tab.
   */
  split?: SplitState
  /** For pruning: the Sessions used least recently go first. */
  usedAt: number
}

export const CONVERSATION: SessionTabId = 'conversation'

/** Sessions remembered at most; older ones are dropped. */
export const MAX_SESSIONS = 100

export function emptyState(now = 0): SessionTabsState {
  return { tabs: [], active: CONVERSATION, usedAt: now }
}

/**
 * One spelling per file (RT-13): `./` goes, and a path under `root` (the
 * Project folder, which the file viewer reads relative paths from) becomes
 * relative. Anything else stays as given.
 */
export function normalizeTabPath(path: string, root?: string | null): string {
  let p = path
  while (p.startsWith('./')) p = p.slice(2)
  const prefix = root ? root.replace(/\/+$/, '') + '/' : null
  if (prefix && p.startsWith(prefix) && p.length > prefix.length) p = p.slice(prefix.length)
  return p || path
}

export function fileTabId(path: string, root?: string | null): `file:${string}` {
  return `file:${normalizeTabPath(path, root)}`
}

/**
 * Merge file tabs that name the same file (e.g. saved before RT-13 as both
 * `tasks/x.md` and `/abs/root/tasks/x.md`) into the first one; it stays pinned
 * when any of them was, and the selection / split side follow it. Returns the
 * same state when nothing changes.
 */
export function dedupeTabs(state: SessionTabsState, root?: string | null): SessionTabsState {
  const moved = new Map<SessionTabId, SessionTabId>()
  const tabs: SessionTab[] = []
  for (const t of state.tabs) {
    if (t.kind !== 'file') {
      tabs.push(t)
      continue
    }
    const path = normalizeTabPath(t.path, root)
    const id = fileTabId(path)
    if (id !== t.id) moved.set(t.id, id)
    const i = tabs.findIndex((o) => o.id === id)
    if (i === -1) tabs.push(id === t.id ? t : { ...t, id, path })
    else if (t.pinned && !tabs[i].pinned) tabs[i] = { ...tabs[i], pinned: true }
  }
  if (!moved.size && tabs.length === state.tabs.length) return state
  const follow = (id: SessionTabId | null) => (id && moved.get(id)) || id
  const split = state.split ? { split: { ...state.split, rightActive: follow(state.split.rightActive) } } : {}
  return { ...state, tabs, active: follow(state.active) as SessionTabId, ...split }
}

/** Pinned tabs right after Conversation, then the rest in the order they were opened. */
export function orderedTabs(state: SessionTabsState): SessionTab[] {
  return [...state.tabs.filter((t) => t.pinned), ...state.tabs.filter((t) => !t.pinned)]
}

/** While split, a file tab picked anywhere is the one shown on the right. */
function rightPick(state: SessionTabsState, id: SessionTabId): { split?: SplitState } {
  const { split } = state
  return split?.on && id !== CONVERSATION ? { split: { ...split, rightActive: id } } : split ? { split } : {}
}

function hasTab(state: SessionTabsState, id: SessionTabId): boolean {
  return id === CONVERSATION || state.tabs.some((t) => t.id === id)
}

/**
 * Open `path` as a tab, or select its tab when it is already open (moving to `line` when given).
 * `root`: the Project folder, so the relative and absolute spellings of a file share one tab.
 */
export function openTab(
  state: SessionTabsState, path: string, line: number | null, now = state.usedAt, root?: string | null,
): SessionTabsState {
  state = dedupeTabs(state, root)
  path = normalizeTabPath(path, root)
  const id = fileTabId(path)
  const existing = state.tabs.find((t) => t.id === id)
  const tabs: SessionTab[] = existing
    ? state.tabs.map((t) => (t.id === id && t.kind === 'file' ? { ...t, line: line ?? t.line } : t))
    : [...state.tabs, { id, kind: 'file', path, line, pinned: false }]
  return { ...state, tabs, active: id, ...rightPick(state, id), usedAt: now }
}

/** Open the "Changed files" tab, or select it when it is already open. */
export function openChangedTab(state: SessionTabsState, now = state.usedAt): SessionTabsState {
  const tabs: SessionTab[] = state.tabs.some((t) => t.id === 'changed')
    ? state.tabs
    : [...state.tabs, { id: 'changed', kind: 'changed', pinned: false }]
  return { ...state, tabs, active: 'changed', ...rightPick(state, 'changed'), usedAt: now }
}

export function selectTab(state: SessionTabsState, id: SessionTabId, now = state.usedAt): SessionTabsState {
  if (!hasTab(state, id)) return state
  return { ...state, active: id, ...rightPick(state, id), usedAt: now }
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
  return fixSplit({ ...state, tabs: state.tabs.filter((t) => t.id !== id), active }, state, id)
}

/**
 * After tabs went away: the right side needs a tab that exists. `before` is the
 * state the change started from; `closedId` the tab closed, so the right side
 * moves to its left neighbour. No tab left → the split ends.
 */
function fixSplit(next: SessionTabsState, before: SessionTabsState, closedId: SessionTabId | null): SessionTabsState {
  const split = next.split
  if (!split?.on) return next
  if (next.tabs.length === 0) return { ...next, active: CONVERSATION, split: { ...split, on: false, rightActive: null } }
  if (split.rightActive && next.tabs.some((t) => t.id === split.rightActive)) return next
  const order = orderedTabs(before)
  const i = closedId ? order.findIndex((t) => t.id === closedId) : -1
  const remaining = new Set(next.tabs.map((t) => t.id as SessionTabId))
  const left = order.slice(0, Math.max(i, 0)).reverse().find((t) => remaining.has(t.id))
  const rightActive = left?.id ?? orderedTabs(next)[0].id
  // `active` followed the right side; keep it so, unless Conversation was selected.
  const active = before.active === CONVERSATION ? CONVERSATION : rightActive
  return { ...next, active, split: { ...split, rightActive } }
}

/** Keep Conversation, the pinned tabs and `id`. */
export function closeOthers(state: SessionTabsState, id: SessionTabId): SessionTabsState {
  const tabs = state.tabs.filter((t) => t.pinned || t.id === id)
  const active = tabs.some((t) => t.id === state.active) || state.active === CONVERSATION
    ? state.active
    : hasTab({ ...state, tabs }, id) ? id : CONVERSATION
  const closed = state.tabs.find((t) => !tabs.includes(t) && t.id === state.split?.rightActive)
  return fixSplit({ ...state, tabs, active }, state, closed ? closed.id : null)
}

export function togglePin(state: SessionTabsState, id: SessionTabId): SessionTabsState {
  if (!state.tabs.some((t) => t.id === id)) return state
  const flipped = { ...state, tabs: state.tabs.map((t) => (t.id === id ? { ...t, pinned: !t.pinned } : t)) }
  // Keep the stored order the shown order, so the tab stays where it landed.
  return { ...flipped, tabs: orderedTabs(flipped) }
}

/**
 * Drag to reorder (UR-33): move `id` to `toIndex` of the shown order (orderedTabs,
 * Conversation not counted). A tab stays in its own group — pinned or not — so
 * `toIndex` is clamped to that group's span; Conversation never moves.
 */
export function moveTab(state: SessionTabsState, id: SessionTabId, toIndex: number): SessionTabsState {
  const order = orderedTabs(state)
  const from = order.findIndex((t) => t.id === id)
  if (from === -1 || !Number.isFinite(toIndex)) return state
  const pinnedCount = order.filter((t) => t.pinned).length
  const [min, max] = order[from].pinned ? [0, pinnedCount - 1] : [pinnedCount, order.length - 1]
  const to = Math.min(max, Math.max(min, Math.trunc(toIndex)))
  if (to === from) return state
  const tabs = order.filter((t) => t.id !== id)
  tabs.splice(to, 0, order[from])
  return { ...state, tabs }
}

/** Close the tab of a deleted file, or the tabs of the files under a deleted folder. */
export function removeTabsUnder(state: SessionTabsState, path: string): SessionTabsState {
  let next = state
  for (const t of state.tabs) {
    if (t.kind === 'file' && (t.path === path || t.path.startsWith(`${path}/`))) next = closeTab(next, t.id)
  }
  return next
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
  const right = state.split?.rightActive
  const rightMoved = right && state.tabs.find((t) => t.id === right && t.kind === 'file' && moved(t.path) !== null)
  const split = state.split && rightMoved && rightMoved.kind === 'file'
    ? { ...state.split, rightActive: fileTabId(moved(rightMoved.path) as string) }
    : state.split
  return { ...state, tabs, active, split }
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
  const path = normalizeTabPath(raw.path)
  return { id: fileTabId(path), kind: 'file', path, line, pinned }
}

/** A stored tab id, spelled as parseTab now spells its tab's id. */
function storedId(raw: unknown): SessionTabId | null {
  if (typeof raw !== 'string') return null
  return raw.startsWith('file:') ? fileTabId(raw.slice(5)) : (raw as SessionTabId)
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
    const wanted = storedId(split.rightActive)
    const right = wanted && hasTab(state, wanted) ? wanted : null
    const ratio = Number.isFinite(split.ratio) ? Math.min(0.9, Math.max(0.1, split.ratio)) : 0.5
    state.split = { on: split.on, ratio, rightActive: right }
  }
  const active = storedId(raw.active)
  if (active && hasTab(state, active)) state.active = active
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

// ── Split view (V3-27) ─────────────────────────────────────────────────────

/** Narrower windows show the tabs layout instead of the split. */
export const SPLIT_MIN_WINDOW = 1100
/** Each side keeps at least this many px. */
export const SPLIT_MIN_SIDE = 360
export const SPLIT_STEP = 0.05

/** Split needs a tab for the right side and a window wide enough. */
export function splitBlockedReason(state: SessionTabsState, windowWidth: number): 'narrow' | 'noFile' | null {
  if (windowWidth < SPLIT_MIN_WINDOW) return 'narrow'
  return state.tabs.length === 0 ? 'noFile' : null
}

/** 'split' only while the split is on, there is a tab to show, and the window is wide enough. */
export function effectiveLayout(state: SessionTabsState, windowWidth: number): 'tabs' | 'split' {
  return state.split?.on && state.tabs.length > 0 && windowWidth >= SPLIT_MIN_WINDOW ? 'split' : 'tabs'
}

/** The left share allowed for `totalPx`: each side keeps SPLIT_MIN_SIDE (too narrow for that → 50 / 50). */
export function ratioLimits(totalPx: number): { min: number; max: number } {
  if (!(totalPx >= SPLIT_MIN_SIDE * 2)) return { min: 0.5, max: 0.5 }
  return { min: SPLIT_MIN_SIDE / totalPx, max: 1 - SPLIT_MIN_SIDE / totalPx }
}

/** The left share for a left side `leftPx` wide, kept within the limits. */
export function clampRatio(leftPx: number, totalPx: number): number {
  const { min, max } = ratioLimits(totalPx)
  if (!Number.isFinite(leftPx)) return 0.5
  return Math.min(max, Math.max(min, leftPx / totalPx))
}

/** Start the split with `tabId` (or the selected file tab, or the first one) on the right. */
export function startSplit(state: SessionTabsState, tabId?: SessionTabId): SessionTabsState {
  if (state.tabs.length === 0) return state
  const pick = [tabId, state.active, state.split?.rightActive].find(
    (id): id is SessionTabId => !!id && id !== CONVERSATION && state.tabs.some((t) => t.id === id),
  )
  const rightActive = pick ?? orderedTabs(state)[0].id
  return { ...state, active: rightActive, split: { on: true, ratio: state.split?.ratio ?? 0.5, rightActive } }
}

/** Back to one tab bar; the tab that was selected on the right stays selected. */
export function stopSplit(state: SessionTabsState): SessionTabsState {
  if (!state.split?.on) return state
  const right = state.split.rightActive
  const active = right && hasTab(state, right) ? right : state.active
  return { ...state, active, split: { ...state.split, on: false } }
}

/** Set the share from a drag or a key: `leftPx` of `totalPx`. Stored only when the split exists. */
export function setRatio(state: SessionTabsState, leftPx: number, totalPx: number): SessionTabsState {
  if (!state.split) return state
  return { ...state, split: { ...state.split, ratio: clampRatio(leftPx, totalPx) } }
}

export function resetRatio(state: SessionTabsState): SessionTabsState {
  return state.split ? { ...state, split: { ...state.split, ratio: 0.5 } } : state
}

/** The tab the file area shows: the right side's while split, else the selected one. */
export function fileAreaTab(state: SessionTabsState, layout: 'tabs' | 'split'): SessionTabId {
  return layout === 'split' ? (state.split?.rightActive ?? state.active) : state.active
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
