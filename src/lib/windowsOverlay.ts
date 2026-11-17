// Pure rules of the "Open windows" overlay (components/windows/WindowsOverlay.vue):
// which windows get a card, how a card reads, filter, grouping and the keyboard
// selection. No Vue / Tauri / `@/` imports so `node --test` can load it
// (src/lib/__tests__/windowsOverlay.test.mjs).

export type WindowKindTag = 'session' | 'file' | 'screen' | 'remote' | 'secret'
export type WindowRunStatus = 'running' | 'waiting' | 'error'

/** What the opener of a window knew about it; the rest is derived from the label. */
export interface WindowMeta {
  projectId?: string
  runId?: string
  /** The folder of a file window, relative to its project. */
  folder?: string
}
export type WindowMetaMap = Record<string, WindowMeta>

export interface WindowCard {
  label: string
  kind: WindowKindTag
  title: string
  sub: string
  projectName?: string
  status?: WindowRunStatus
  minimized: boolean
  isHere: boolean
  canMerge: boolean
}

export interface WindowInfo {
  label: string
  title: string
  minimized: boolean
}

export interface CardContext {
  /** The display name of a window (lib/tray `labelFor`). */
  nameOf: (label: string, title: string) => string
  /** The Project name of a project id, '' when unknown. */
  projectNameOf: (projectId: string) => string
  /** The second line of a window that is a whole screen (Calendar, Gantt…) or a remote one. */
  screenSub: (label: string, title: string) => string
}

export type MoveDirection = 'left' | 'right' | 'up' | 'down'
export type GroupBy = 'type' | 'project'

export interface CardGroup {
  /** `session` / `remote` / `screen` for 'type'; the Project name for 'project'; `none` for no Project. */
  key: string
  isNone: boolean
  cards: WindowCard[]
}

/** Broadcast whenever a window opens or closes: every window recounts. */
export const WINDOWS_CHANGED = 'windows:changed'
/** `{ target }`: the window with that label opens its overlay (every window hears it, only the target acts). */
export const WINDOWS_OVERLAY_OPEN = 'windows:overlay-open'
/** `{ target }`: the window with that label does what its "Move back to main window" button does. */
export const WINDOW_MERGE_REQUEST = 'window:merge-request'
export interface WindowTargeted {
  target: string
}

export const META_STORAGE_KEY = 'puppeteer.windowMeta'
export const RECENT_STORAGE_KEY = 'puppeteer.windowRecent'
export const RUN_STATUS_STORAGE_KEY = 'puppeteer.windowRunStatus'
export const GROUP_STORAGE_KEY = 'puppeteer.windowOverlayGroup'
const MAX_RECENT = 20

/** Windows that never get a card: the main one and the two transient ones. */
function isExcluded(label: string): boolean {
  const own = label.startsWith('remote-') && label !== 'remote-desktop' ? label.slice('remote-'.length) : label
  return (
    label === 'main' ||
    label === 'remote-bridge' ||
    own === 'permission-prompt' ||
    own.startsWith('permission-')
  )
}

/** The kind of a window by its label, or null when it is not a "private window" (no card). */
export function kindOfLabel(label: string): WindowKindTag | null {
  if (isExcluded(label)) return null
  // The remote-desktop window and the windows it opens (lib/ipc/windows.ts).
  if (label.startsWith('remote-')) return 'remote'
  if (label.startsWith('session-')) return 'session'
  if (label.startsWith('fileviewer-')) return 'file'
  if (label === 'secret-notes') return 'secret'
  return 'screen'
}

/** Screen windows (fixed label, or a prefix) whose own header has "Move back to main window". */
const MERGE_BACK_LABELS: readonly string[] = [
  'calendar', 'gantt', 'pr-inbox', 'settings', 'focus', 'slack-threads', 'secret-notes', 'file-transfer',
  // The todo list window has no such button, only the notes one.
  'item-list-note',
]

/**
 * Whether "Move back to main window" works for this window: only the windows that really have the
 * button. A file viewer has it only when a session opened it (`meta.runId`).
 */
export function canMergeBack(label: string, meta?: WindowMeta): boolean {
  const kind = kindOfLabel(label)
  if (kind === 'session') return true
  if (kind === 'file') return !!meta?.runId
  return MERGE_BACK_LABELS.includes(label)
}

// The window's own "Move back" control (MergeBackButton, the session header, the file viewer) registers
// what it does, so the overlay of another window runs the SAME function: unsaved-edit prompt, the
// section or item being viewed, all kept.
let mergeBackAction: (() => Promise<void>) | null = null

/** Register this window's "Move back to main window"; returns the function that unregisters it. */
export function registerMergeBack(run: () => Promise<void>): () => void {
  mergeBackAction = run
  return () => {
    if (mergeBackAction === run) mergeBackAction = null
  }
}

/** Run this window's "Move back to main window". False when this window registered none. */
export async function runMergeBack(): Promise<boolean> {
  if (!mergeBackAction) return false
  await mergeBackAction()
  return true
}

interface CloseKeyEvent {
  key: string
  code: string
  metaKey: boolean
  ctrlKey: boolean
  shiftKey: boolean
  altKey: boolean
}

/** Delete / ⌘W (Ctrl+W) closes the selected card; Delete is a text key while the filter has text in it. */
export function isCloseCardKey(ev: CloseKeyEvent, isMac: boolean, typingInFilter: boolean): boolean {
  if (ev.key === 'Delete') return !typingInFilter
  return (isMac ? ev.metaKey : ev.ctrlKey) && !ev.shiftKey && !ev.altKey && ev.code === 'KeyW'
}

/** One card per private window, in label order. */
export function cardsFrom(
  windows: readonly WindowInfo[],
  meta: WindowMetaMap,
  statuses: Readonly<Record<string, WindowRunStatus>>,
  hereLabel: string,
  ctx: CardContext,
): WindowCard[] {
  const cards: WindowCard[] = []
  for (const w of windows) {
    const kind = kindOfLabel(w.label)
    if (!kind) continue
    const base = {
      label: w.label, kind, minimized: w.minimized, isHere: w.label === hereLabel,
      canMerge: canMergeBack(w.label, meta[w.label]),
    }
    // The vault never shows anything of its content: no OS title, no meta, no status (QĐ-18).
    if (kind === 'secret') {
      cards.push({ ...base, title: ctx.nameOf(w.label, ''), sub: '' })
      continue
    }
    const m = meta[w.label] ?? {}
    const projectName = m.projectId ? ctx.projectNameOf(m.projectId) : ''
    const title = ctx.nameOf(w.label, w.title)
    let sub = ''
    if (kind === 'session') sub = projectName
    else if (kind === 'file') sub = [projectName, m.folder].filter(Boolean).join(' · ')
    else sub = ctx.screenSub(w.label, w.title)
    const status = kind === 'session' && m.runId ? statuses[m.runId] : undefined
    cards.push({ ...base, title, sub, ...(projectName ? { projectName } : {}), ...(status ? { status } : {}) })
  }
  return cards.sort((a, b) => a.label.localeCompare(b.label))
}

/** Lower-case, no accents (Vietnamese "đ" included), for matching what the user typed. */
export function fold(text: string): string {
  return text
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/đ/g, 'd')
    .replace(/Đ/g, 'd')
    .toLowerCase()
}

export function filterCards(cards: readonly WindowCard[], query: string): WindowCard[] {
  const q = fold(query.trim())
  if (!q) return [...cards]
  return cards.filter((c) => fold(c.title).includes(q))
}

const TYPE_ORDER = ['session', 'remote', 'screen']

function typeKey(kind: WindowKindTag): string {
  return kind === 'session' ? 'session' : kind === 'remote' ? 'remote' : 'screen'
}

/**
 * Blocks of the overlay. By type: the blocks with fewer cards first (so the first row
 * is full), ties in the fixed order. By Project: by name, "no Project" last.
 */
export function groupCards(cards: readonly WindowCard[], by: GroupBy): CardGroup[] {
  const map = new Map<string, WindowCard[]>()
  for (const c of cards) {
    const key = by === 'type' ? typeKey(c.kind) : c.projectName || ''
    const list = map.get(key) ?? []
    list.push(c)
    map.set(key, list)
  }
  const groups: CardGroup[] = [...map.entries()].map(([key, list]) => ({
    key: by === 'project' && key === '' ? 'none' : key,
    isNone: by === 'project' && key === '',
    cards: list,
  }))
  if (by === 'type') {
    return groups.sort(
      (a, b) => a.cards.length - b.cards.length || TYPE_ORDER.indexOf(a.key) - TYPE_ORDER.indexOf(b.key),
    )
  }
  return groups.sort((a, b) => Number(a.isNone) - Number(b.isNone) || a.key.localeCompare(b.key))
}

/** Cards in the order they show on screen. */
export function flattenGroups(groups: readonly CardGroup[]): WindowCard[] {
  return groups.flatMap((g) => g.cards)
}

/**
 * The card selected when the overlay opens. From a Session window: the session waiting
 * for the user, else the window used last (other than this one). From any other: the
 * window used last. `recent` is most recent first.
 */
export function initialSelection(
  cards: readonly WindowCard[],
  here: string,
  recent: readonly string[],
): string | null {
  if (cards.length === 0) return null
  const others = cards.filter((c) => c.label !== here)
  const pool = others.length ? others : cards
  if (here.startsWith('session-') || here.startsWith('remote-session-')) {
    const waiting = pool.find((c) => c.status === 'waiting')
    if (waiting) return waiting.label
  }
  for (const label of recent) {
    const hit = pool.find((c) => c.label === label)
    if (hit) return hit.label
  }
  return pool[0].label
}

/** Arrow keys over the cards (display order). Up / down jump a row of `columns`; the ends stay put. */
export function moveSelection(
  cards: readonly WindowCard[],
  selected: string | null,
  direction: MoveDirection,
  columns: number,
): string | null {
  if (cards.length === 0) return null
  const i = cards.findIndex((c) => c.label === selected)
  if (i < 0) return cards[0].label
  const step = direction === 'left' ? -1 : direction === 'right' ? 1 : direction === 'up' ? -columns : columns
  const next = i + step
  if (next < 0 || next >= cards.length) return cards[i].label
  return cards[next].label
}

/** ⌘⇧E again while the overlay is open: the next card, back to the first after the last. */
export function cycleSelection(cards: readonly WindowCard[], selected: string | null): string | null {
  if (cards.length === 0) return null
  const i = cards.findIndex((c) => c.label === selected)
  return cards[(i + 1) % cards.length].label
}

/** The card to select after `label` closed: the next one, else the previous. */
export function selectionAfterClose(cards: readonly WindowCard[], label: string): string | null {
  const i = cards.findIndex((c) => c.label === label)
  if (i < 0) return null
  return (cards[i + 1] ?? cards[i - 1])?.label ?? null
}

/** `recent` with `label` moved to the front. */
export function touchRecent(recent: readonly string[], label: string): string[] {
  return [label, ...recent.filter((l) => l !== label)].slice(0, MAX_RECENT)
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return !!v && typeof v === 'object' && !Array.isArray(v)
}

function parseJson(raw: string | null): unknown {
  if (!raw) return null
  try {
    return JSON.parse(raw)
  } catch {
    return null
  }
}

/** The stored window meta; a broken value is an empty map. Only known string fields are kept. */
export function parseMeta(raw: string | null): WindowMetaMap {
  const data = parseJson(raw)
  if (!isRecord(data)) return {}
  const out: WindowMetaMap = {}
  for (const [label, value] of Object.entries(data)) {
    if (!isRecord(value)) continue
    const m: WindowMeta = {}
    if (typeof value.projectId === 'string') m.projectId = value.projectId
    if (typeof value.runId === 'string') m.runId = value.runId
    if (typeof value.folder === 'string') m.folder = value.folder
    out[label] = m
  }
  return out
}

export function parseRecent(raw: string | null): string[] {
  const data = parseJson(raw)
  return Array.isArray(data) ? data.filter((x): x is string => typeof x === 'string').slice(0, MAX_RECENT) : []
}

const RUN_STATUSES: readonly string[] = ['running', 'waiting', 'error']

export function parseRunStatuses(raw: string | null): Record<string, WindowRunStatus> {
  const data = parseJson(raw)
  if (!isRecord(data)) return {}
  const out: Record<string, WindowRunStatus> = {}
  for (const [runId, status] of Object.entries(data)) {
    if (typeof status === 'string' && RUN_STATUSES.includes(status)) out[runId] = status as WindowRunStatus
  }
  return out
}

/** The badge of a run in the main window's live state: waiting for the user beats running; null = no badge. */
export function runStatusOf(session: { status: string; pending: number }): WindowRunStatus | null {
  if (session.pending > 0) return 'waiting'
  if (session.status === 'running') return 'running'
  if (session.status === 'failed') return 'error'
  return null
}

export function parseGroupBy(raw: string | null): GroupBy {
  return raw === 'project' ? 'project' : 'type'
}
