<script setup lang="ts">
import { ref, computed, watch, onMounted, onBeforeUnmount, toRaw, type Component } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  ChevronRight, Wrench, Sparkles, CheckCircle2, XCircle, Cpu, Brain, AlertTriangle, Info,
  Terminal, FilePen, FilePlus2, FileText, Search, Globe, ListTodo, User, Archive, Copy, Check,
  Bot, MessageCircleQuestion, Circle, CircleDot, ClipboardList, Server, BookOpen, BellRing,
} from 'lucide-vue-next'
import type { LiveStatus, StreamEntry, TaskProgress } from '@/lib/streamEvents'
import { vMermaid } from '@/lib/mermaid'
import { vCopyCode } from '@/lib/copyCode'
import { computeDiff } from '@/lib/diff'
import { toolFileTarget, toolFileTargets, codexEditParts, type DiffPart } from '@/lib/toolFiles'
import IdeContextChip from './IdeContextChip.vue'
import DiffView from './DiffView.vue'
import MsgActions, { type MsgActionsState } from './sessions/MsgActions.vue'
import MessageFullscreen from './sessions/MessageFullscreen.vue'
import TurnActivityHeader from './sessions/TurnActivityHeader.vue'
import { CollapsibleMessage } from '@/components/ui'
import { pickSurface } from '@/lib/selectionSurface'
import { selectionHost, selectionQuoteNote, selectionSurfaces, surfaceEls } from '@/composables/useSelectionSurface'
import { turnAnswerText, turnFooters, turnPromptAt, type TurnAction, type TurnFooter } from '@/lib/turnActions'
import {
  activityMatches, activityOpen, formatActivityDuration, turnActivity, type TurnActivity, type TurnActivityMap,
} from '@/lib/turnActivity'
import { stripQuotePrefix, type QuoteCard, type QuoteSetView, type RunQuote } from '@/lib/quote'
import { QUOTE_BLOCK_ATTR, QUOTE_SRC_ATTR, type QuoteAnchor } from '@/lib/quoteAnchor'
import { hrefFileRef, parseLineRef, splitLineRef } from '@/lib/fileLinks'

const { t } = useI18n()

const props = defineProps<{
  entries: StreamEntry[]
  running?: boolean
  /** An API retry or compaction in progress: shown in place of "thinking". */
  liveStatus?: LiveStatus | null
  /** `skipCache` bypasses the memo for text that is still growing — see below. */
  renderText: (md: string, skipCache?: boolean) => string
  /** Resolve a raw inline-code token to a project file path, or null. */
  fileMatcher?: (raw: string) => string | null
  /** Whether a link the matcher doesn't know names a real file (asked on click). */
  fileExists?: (path: string) => Promise<boolean>
  /** Bumped when the project file list reloads, so finalized entries re-link. */
  fileListVersion?: number
  /** No row of buttons under each turn (the whole-turn fullscreen nests a log). */
  noTurnActions?: boolean
  /** Fold each turn's tool calls, thinking and commentary under one header. */
  groupSteps?: boolean
  /** The viewer's "expand all": every block starts open, not only the running turn's. */
  stepsExpanded?: boolean
  /** The ⌘F query while the bar is open, so a folded block holding a match opens. */
  findQuery?: string
  findCaseSensitive?: boolean
  /** Structured quotes of the prompts here, by `quoteSet` (UX-29): cards on the bubble. */
  quoteSets?: ReadonlyMap<string, QuoteSetView>
  /** A quote's source being jumped to: its message (id) opens if folded. `seq` re-triggers. */
  quoteReveal?: { id: string; seq: number } | null
}>()

const emit = defineEmits<{
  (e: 'open-file', path: string, line?: number | null): void
  (e: 'open-url', url: string): void
  (e: 'quote-jump', quote: RunQuote): void
}>()

// Per-entry expand state for compaction-summary dividers (collapsed by default).
const expandedCompacts = ref(new Set<number>())
function toggleCompact(i: number) {
  const next = new Set(expandedCompacts.value)
  next.has(i) ? next.delete(i) : next.add(i)
  expandedCompacts.value = next
}

// Directive: mark inline `<code>` tokens that name a real project file so they
// render as clickable file links (handled by the container's click listener).
const vFileLinks = {
  mounted: (el: HTMLElement) => decorateFileLinks(el),
  updated: (el: HTMLElement) => decorateFileLinks(el),
}

function decorateFileLinks(el: HTMLElement) {
  const match = props.fileMatcher
  if (!match) return
  el.querySelectorAll('code').forEach((code) => {
    if (code.closest('pre')) return // skip multi-line code blocks
    const path = match((code.textContent ?? '').trim())
    if (path) {
      code.classList.add('file-link')
      code.setAttribute('data-file-path', path)
    } else {
      code.classList.remove('file-link')
      code.removeAttribute('data-file-path')
    }
  })
}

function onProseClick(e: MouseEvent) {
  const target = e.target as HTMLElement
  // Inline `<code>` that names a project file.
  const code = target.closest('code.file-link') as HTMLElement | null
  const codePath = code?.getAttribute('data-file-path')
  if (codePath) {
    e.preventDefault()
    emit('open-file', codePath, parseLineRef(code?.textContent ?? ''))
    return
  }
  // Markdown link: never let the webview follow it (a relative href blanks the
  // whole SPA). Open project files in the viewer, external URLs in the browser.
  const anchor = target.closest('a') as HTMLAnchorElement | null
  if (anchor) {
    e.preventDefault()
    const href = anchor.getAttribute('href') ?? ''
    if (!href) return
    if (/^(https?:|mailto:)/i.test(href)) {
      emit('open-url', href)
      return
    }
    const ref = hrefFileRef(href)
    if (!ref) return
    const path = props.fileMatcher?.(ref.path) ?? null
    if (path) {
      emit('open-file', path, ref.line)
      return
    }
    // Not in the project list (outside the project, ignored, or created after
    // the list loaded): open it only when it is a real file.
    void props.fileExists?.(ref.path).then((ok) => {
      if (ok) emit('open-file', ref.path, ref.line)
    })
  }
}

// Return the file path a tool acts on, or null when the tool isn't file-based.
// Engine-agnostic: Claude's `file_path` and Codex's `fileChange` / `apply_patch`
// payloads both resolve here (see `@/lib/toolFiles`).
function fileTarget(input: unknown): string | null {
  return toolFileTarget(input)
}

// How many *extra* files the same call touches — Codex can patch several files
// in one `fileChange` item, and only the first fits in the header.
function extraFileCount(input: unknown): number {
  return Math.max(toolFileTargets(input).length - 1, 0)
}

// Per-tool visual identity: distinct icon + accent color per tool category,
// so exec/edit/write/read/search stand out and read apart from prose.
interface ToolStyle {
  icon: Component
  iconClass: string
  nameClass: string
  barClass: string // left accent border color
}

const DEFAULT_TOOL_STYLE: ToolStyle = {
  icon: Wrench,
  iconClass: 'text-indigo-400/80',
  nameClass: 'text-indigo-500 dark:text-indigo-300/90',
  barClass: 'border-l-indigo-500/50',
}

function toolStyle(name: string): ToolStyle {
  const n = (name || '').toLowerCase()
  if (n === 'mcp' || n.startsWith('mcp__'))
    return { icon: Server, iconClass: 'text-teal-400', nameClass: 'text-teal-600 dark:text-teal-300', barClass: 'border-l-teal-500/60' }
  if (n === 'bash' || n === 'bashoutput' || n === 'killshell' || n === 'killbash')
    return { icon: Terminal, iconClass: 'text-emerald-400', nameClass: 'text-emerald-600 dark:text-emerald-300', barClass: 'border-l-emerald-500/60' }
  if (n === 'edit' || n === 'multiedit' || n === 'notebookedit')
    return { icon: FilePen, iconClass: 'text-amber-400', nameClass: 'text-amber-600 dark:text-amber-300', barClass: 'border-l-amber-500/60' }
  if (n === 'write')
    return { icon: FilePlus2, iconClass: 'text-rose-400', nameClass: 'text-rose-600 dark:text-rose-300', barClass: 'border-l-rose-500/60' }
  if (n === 'read')
    return { icon: FileText, iconClass: 'text-sky-400', nameClass: 'text-sky-600 dark:text-sky-300', barClass: 'border-l-sky-500/60' }
  if (n === 'grep' || n === 'glob')
    return { icon: Search, iconClass: 'text-violet-400', nameClass: 'text-violet-600 dark:text-violet-300', barClass: 'border-l-violet-500/60' }
  if (n === 'webfetch' || n === 'websearch')
    return { icon: Globe, iconClass: 'text-cyan-400', nameClass: 'text-cyan-600 dark:text-cyan-300', barClass: 'border-l-cyan-500/60' }
  if (n === 'todowrite')
    return { icon: ListTodo, iconClass: 'text-blue-400', nameClass: 'text-blue-600 dark:text-blue-300', barClass: 'border-l-blue-500/60' }
  if (n === 'task' || n === 'agent')
    return { icon: Bot, iconClass: 'text-fuchsia-400', nameClass: 'text-fuchsia-600 dark:text-fuchsia-300', barClass: 'border-l-fuchsia-500/60' }
  if (n === 'askuserquestion')
    return { icon: MessageCircleQuestion, iconClass: 'text-blue-400', nameClass: 'text-blue-600 dark:text-blue-300', barClass: 'border-l-blue-500/60' }
  if (n === 'toolsearch')
    return { icon: Search, iconClass: 'text-teal-400', nameClass: 'text-teal-600 dark:text-teal-300', barClass: 'border-l-teal-500/60' }
  if (n === 'exitplanmode')
    return { icon: ClipboardList, iconClass: 'text-indigo-400', nameClass: 'text-indigo-600 dark:text-indigo-300', barClass: 'border-l-indigo-500/60' }
  return DEFAULT_TOOL_STYLE
}

// Bash-family tools get a dedicated terminal-style renderer (command + output),
// instead of the generic raw-JSON input dump used for other tools.
const BASH_TOOLS = new Set(['bash', 'bashoutput', 'killshell', 'killbash'])
function isBash(name: string): boolean {
  return BASH_TOOLS.has((name || '').toLowerCase())
}

interface BashInfo {
  command: string
  description: string
  timeout?: number
  background: boolean
}

// Normalize a Bash tool `input` (Claude sends an object with command/description;
// Codex sends `{ command }` where command may itself be an object) into the
// fields the terminal card renders. Memoized per input object: the template
// reads bashInfo several times per entry and the entries array is stable.
const bashInfoCache = new WeakMap<object, BashInfo>()
function bashInfo(input: unknown): BashInfo {
  if (input && typeof input === 'object') {
    const hit = bashInfoCache.get(input)
    if (hit) return hit
    const info = computeBashInfo(input)
    bashInfoCache.set(input, info)
    return info
  }
  return computeBashInfo(input)
}
function computeBashInfo(input: unknown): BashInfo {
  if (typeof input === 'string') return { command: input, description: '', background: false }
  if (!input || typeof input !== 'object') return { command: String(input ?? ''), description: '', background: false }
  const obj = input as Record<string, unknown>
  const rawCmd = obj.command ?? obj.cmd ?? obj.shell_command ?? ''
  const command = typeof rawCmd === 'string' ? rawCmd : JSON.stringify(rawCmd, null, 2)
  const description = typeof obj.description === 'string' ? obj.description : ''
  const timeout = typeof obj.timeout === 'number' ? obj.timeout : undefined
  const background = obj.run_in_background === true
  return { command, description, timeout, background }
}

// The header's primary line: purpose (description) when available, else the
// command itself. Shell-management tools (BashOutput / KillShell / KillBash)
// carry no command — they reference a background shell by id — so fall back to
// a readable label instead of showing a blank line.
function bashHeadline(name: string, input: unknown): string {
  const { description, command } = bashInfo(input)
  if (description) return description
  if (command) return command
  const o = asObj(input)
  const shellRef = asStr(o.bash_id) || asStr(o.shell_id)
  const n = (name || '').toLowerCase()
  if (n === 'bashoutput') return shellRef ? t('streamLog.readOutputWithRef', { ref: shellRef }) : t('streamLog.readShellOutput')
  if (n === 'killshell' || n === 'killbash') return shellRef ? t('streamLog.killShellWithRef', { ref: shellRef }) : t('streamLog.killShell')
  return name || t('streamLog.fallbackCommand')
}

// Per-entry "copied" flash for the command copy button.
const copiedCmd = ref<Record<number, boolean>>({})
async function copyCommand(i: number, input: unknown) {
  try {
    await navigator.clipboard.writeText(bashInfo(input).command)
    copiedCmd.value[i] = true
    setTimeout(() => { copiedCmd.value[i] = false }, 1500)
  } catch {
    // Clipboard denied — ignore silently.
  }
}

// File-editing tools render as a unified diff instead of a raw-JSON dump.
const EDIT_TOOLS = new Set(['edit', 'multiedit', 'write'])
function isEditTool(name: string): boolean {
  return EDIT_TOOLS.has((name || '').toLowerCase())
}

// Extract the before/after pair(s) a file-editing tool changes: Edit → one pair,
// MultiEdit → one per edit, Write → whole content as an all-added block, Codex
// `fileChange` → one per patch hunk, tagged with the file it belongs to.
// Memoized per input object: the template re-reads this while rendering each
// part (to decide where a new per-file heading starts), and parsing a patch on
// every one of those reads would be quadratic for a large diff.
const editPartsCache = new WeakMap<object, DiffPart[]>()
function editParts(name: string, input: unknown): DiffPart[] {
  if (!input || typeof input !== 'object') return []
  const cached = editPartsCache.get(input)
  if (cached) return cached
  const parts = computeEditParts(name, input)
  editPartsCache.set(input, parts)
  return parts
}

function computeEditParts(name: string, input: unknown): DiffPart[] {
  if (!input || typeof input !== 'object') return []
  const obj = input as Record<string, unknown>
  const n = (name || '').toLowerCase()
  const str = (v: unknown): string => (typeof v === 'string' ? v : '')
  if (obj.changes) return codexEditParts(obj)
  if (n === 'edit') {
    const before = str(obj.old_string)
    const after = str(obj.new_string)
    return before || after ? [{ before, after }] : []
  }
  if (n === 'multiedit' && Array.isArray(obj.edits)) {
    return (obj.edits as unknown[])
      .map((e) => (e && typeof e === 'object' ? (e as Record<string, unknown>) : {}))
      .map((e) => ({ before: str(e.old_string), after: str(e.new_string) }))
      .filter((d) => d.before || d.after)
  }
  if (n === 'write') {
    const after = str(obj.content)
    return after ? [{ before: '', after }] : []
  }
  return []
}

// Aggregate +added / −removed across all diff parts for the header badge.
// Memoized per input object: the template reads editStats up to four times per
// entry and each call runs an O(n·m) LCS — computing it once per entry matters
// for MultiEdit calls with many parts.
const editStatsCache = new WeakMap<object, { added: number; removed: number }>()
function editStats(name: string, input: unknown): { added: number; removed: number } {
  if (!input || typeof input !== 'object') return { added: 0, removed: 0 }
  const hit = editStatsCache.get(input)
  if (hit) return hit
  const stats = editParts(name, input).reduce(
    (acc, p) => {
      const d = computeDiff(p.before, p.after, { fold: false })
      return { added: acc.added + d.added, removed: acc.removed + d.removed }
    },
    { added: 0, removed: 0 },
  )
  editStatsCache.set(input, stats)
  return stats
}

// Small helpers for structured tool inputs below.
function asObj(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {}
}
function asStr(v: unknown): string {
  return typeof v === 'string' ? v : ''
}

// --- Agent (Task): a delegated sub-agent run --------------------------------
function isAgentTool(name: string): boolean {
  const n = (name || '').toLowerCase()
  return n === 'task' || n === 'agent'
}
function agentInfo(input: unknown): { subagent: string; description: string; prompt: string } {
  const o = asObj(input)
  return { subagent: asStr(o.subagent_type), description: asStr(o.description), prompt: asStr(o.prompt) }
}

// --- AskUserQuestion --------------------------------------------------------
interface AskOption { label: string; description: string }
interface AskQuestion { question: string; header: string; multiSelect: boolean; options: AskOption[] }
function isAskUserTool(name: string): boolean {
  return (name || '').toLowerCase() === 'askuserquestion'
}
function askQuestions(input: unknown): AskQuestion[] {
  const raw = asObj(input).questions
  if (!Array.isArray(raw)) return []
  return raw.map((q) => {
    const qo = asObj(q)
    const options = Array.isArray(qo.options)
      ? (qo.options as unknown[]).map((x) => ({ label: asStr(asObj(x).label), description: asStr(asObj(x).description) })).filter((x) => x.label)
      : []
    return { question: asStr(qo.question), header: asStr(qo.header), multiSelect: qo.multiSelect === true, options }
  })
}

// --- ToolSearch -------------------------------------------------------------
function isToolSearchTool(name: string): boolean {
  return (name || '').toLowerCase() === 'toolsearch'
}
function toolSearchQuery(input: unknown): string {
  return asStr(asObj(input).query)
}

// --- TodoWrite --------------------------------------------------------------
interface TodoItem { content: string; status: string }
function isTodoTool(name: string): boolean {
  return (name || '').toLowerCase() === 'todowrite'
}
function todoItems(input: unknown): TodoItem[] {
  const raw = asObj(input).todos
  if (!Array.isArray(raw)) return []
  return (raw as unknown[]).map((t) => ({ content: asStr(asObj(t).content), status: asStr(asObj(t).status) || 'pending' }))
}
function todoStats(input: unknown): { done: number; total: number } {
  const items = todoItems(input)
  return { done: items.filter((t) => t.status === 'completed').length, total: items.length }
}

// --- Web (fetch/search): render the fetched content as markdown -------------
function isWebTool(name: string): boolean {
  const n = (name || '').toLowerCase()
  return n === 'webfetch' || n === 'websearch'
}

// --- MCP --------------------------------------------------------------------
function isMcpTool(name: string): boolean {
  const n = (name || '').toLowerCase()
  return n === 'mcp' || n.startsWith('mcp__')
}
function titleCaseWords(raw: string): string {
  return raw
    .replace(/[_-]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .replace(/\b\w/g, (m) => m.toUpperCase())
}
function mcpRawParts(name: string): { server: string; action: string } {
  const parts = (name || '').split('__')
  if (parts.length >= 3 && parts[0].toLowerCase() === 'mcp') {
    return { server: parts[1], action: parts.slice(2).join('__') }
  }
  return { server: '', action: '' }
}
function mcpServerLabel(entry: Extract<StreamEntry, { kind: 'tool' }>): string {
  const inputServer = asStr(asObj(entry.input).serverName)
  return entry.serverDisplayName || inputServer || mcpRawParts(entry.name).server || 'MCP'
}
function mcpActionLabel(entry: Extract<StreamEntry, { kind: 'tool' }>): string {
  if (entry.displayName) return entry.displayName
  const inputTool = asStr(asObj(entry.input).toolName)
  const rawAction = inputTool || mcpRawParts(entry.name).action
  return rawAction ? titleCaseWords(rawAction) : t('streamLog.mcpRequest')
}
function mcpSummary(entry: Extract<StreamEntry, { kind: 'tool' }>): string {
  const input = asObj(entry.input)
  const message = asStr(input.message)
  if (message) return message
  const questions = input.questions
  if (Array.isArray(questions)) {
    const first = asStr(asObj(questions[0]).question)
    if (first) return first
  }
  return asStr(input.mode) || previewInput(entry.input)
}

// --- ExitPlanMode: Claude presenting a plan to leave plan mode --------------
// Render the proposed plan as markdown, not a raw tool-input dump.
function isPlanTool(name: string): boolean {
  return (name || '').toLowerCase() === 'exitplanmode'
}
function planContent(input: unknown): string {
  return asStr(asObj(input).plan)
}

// Only overly long user messages get collapsed; assistant replies always render
// in full. The most recent user turn is what the user is actively working with,
// so it stays expanded — older user turns collapse if they overflow.
const expandedMessageIndices = computed(() => {
  const indices = new Set<number>()
  for (let i = props.entries.length - 1; i >= 0; i--) {
    if (props.entries[i].kind === 'user') {
      indices.add(i)
      break
    }
  }
  return indices
})

const expanded = ref<Record<number, boolean>>({})

watch(
  () => props.entries.length,
  () => {
    for (let i = 0; i < props.entries.length; i++) {
      const e = props.entries[i]
      if (e.kind === 'tool' && expanded.value[i] === undefined) {
        // Plans are meant to be read — show them open; other tools start collapsed
        // unless they errored.
        expanded.value[i] = isPlanTool(e.name) || !!e.result?.is_error
      }
    }
  },
  { immediate: true }
)

function toggle(i: number) {
  expanded.value[i] = !expanded.value[i]
}

// A run of user text: either plain prose or an `@file` mention that resolves to
// a real project file (and is therefore clickable to open in the viewer).
interface TextSegment {
  text: string
  path?: string
  line?: number | null
}

// Clickable file token in a user message, matched as either:
//   1. A backticked absolute path — `/Users/.../file.md` — produced by the
//      drag-drop composer's file-attachment block. Always clickable.
//   2. An `@`-mention — `@src/foo.ts` — clickable only when fileMatcher resolves
//      it to an actual project file.
const FILE_TOKEN_RE = /`(\/[^`\n]+)`|@(\S+)/g

// Split a user message into plain + clickable segments. Backticked absolute
// paths (dropped-file attachments) become links regardless of the project;
// `@tokens` become links only when they resolve to a project file. Everything
// else (emails, `@` in prose, unknown paths) stays plain text.
function userSegments(text: string): TextSegment[] {
  const match = props.fileMatcher
  const segs: TextSegment[] = []
  let last = 0
  FILE_TOKEN_RE.lastIndex = 0
  let m: RegExpExecArray | null
  while ((m = FILE_TOKEN_RE.exec(text))) {
    // Branch 1: backticked absolute path — attachment reference, always linked.
    if (m[1] !== undefined) {
      if (m.index > last) segs.push({ text: text.slice(last, m.index) })
      const ref = splitLineRef(m[1])
      segs.push({ text: m[1], path: ref.path, line: ref.line })
      last = m.index + m[0].length
      continue
    }
    // Branch 2: `@`-mention — link only when it resolves to a project file.
    const raw = m[2]
    if (!match) continue
    // Keep trailing punctuation (`,` `.` `)`…) outside the link, matching how
    // fileMatcher itself trims it when resolving the path.
    const core = raw.replace(/[)\].,:;'"`]+$/, '')
    const path = match(core)
    if (!path) continue
    if (m.index > last) segs.push({ text: text.slice(last, m.index) })
    segs.push({ text: '@' + core, path, line: parseLineRef(core) })
    const trailing = raw.slice(core.length)
    if (trailing) segs.push({ text: trailing })
    last = m.index + m[0].length
  }
  if (last < text.length) segs.push({ text: text.slice(last) })
  return segs.length ? segs : [{ text }]
}

function previewInput(input: unknown): string {
  if (input == null) return ''
  if (typeof input === 'string') return input
  if (typeof input === 'object') {
    const obj = input as Record<string, unknown>
    const preferred = ['file_path', 'path', 'command', 'pattern', 'query', 'url']
    for (const k of preferred) {
      if (typeof obj[k] === 'string') return obj[k] as string
    }
    const json = JSON.stringify(obj)
    return json.length > 80 ? json.slice(0, 80) + '…' : json
  }
  return String(input)
}

function prettyInput(input: unknown): string {
  if (input == null) return ''
  if (typeof input === 'string') return input
  try {
    return JSON.stringify(input, null, 2)
  } catch {
    return String(input)
  }
}

// Muted badge colors per diagnostic level — calm, not alarming.
function logBadgeClass(level: string): string {
  switch (level) {
    case 'warn': return 'text-warning bg-warning/10'
    case 'error': return 'text-destructive bg-destructive/10'
    case 'debug':
    case 'trace': return 'text-foreground/40 bg-foreground/5'
    default: return 'text-info bg-info/10'
  }
}

function formatDuration(ms?: number): string {
  if (!ms) return ''
  if (ms < 1000) return `${ms}ms`
  return `${(ms / 1000).toFixed(1)}s`
}

function formatTokens(n?: number): string {
  if (!n) return '0'
  if (n < 1000) return String(n)
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}k`
  return `${(n / 1_000_000).toFixed(2)}M`
}

const lastEntryIsResult = computed(() => {
  const last = props.entries[props.entries.length - 1]
  return last?.kind === 'result'
})

const liveStatusText = computed(() => {
  const s = props.liveStatus
  if (!s) return ''
  if (s.kind === 'compacting') return t('streamLog.live.compacting')
  const args = { attempt: s.attempt, max: s.maxRetries, secs: Math.ceil(s.delayMs / 1000) }
  return s.errorStatus !== null
    ? t('streamLog.live.retryStatus', { ...args, status: s.errorStatus })
    : t('streamLog.live.retry', args)
})

// `tool_progress` counts whole seconds; `12.0s` would read as false precision.
function formatElapsed(secs: number): string {
  return secs < 60 ? `${secs}s` : formatActivityDuration(secs * 1000)
}

/** A running tool card's status: how long it has run, once the engine says. */
function runningText(e: StreamEntry, idle: string): string {
  return e.kind === 'tool' && e.elapsedSecs ? t('streamLog.runningFor', { time: formatElapsed(e.elapsedSecs) }) : idle
}

function taskProgressText(task: TaskProgress): string {
  const parts: string[] = []
  if (task.toolUses) parts.push(t('streamLog.taskTools', task.toolUses))
  if (task.tokens) parts.push(t('streamLog.tokensSuffix', { tokens: formatTokens(task.tokens) }))
  if (task.durationMs) parts.push(formatActivityDuration(task.durationMs))
  if (task.lastTool && !task.done) parts.push(task.lastTool)
  return parts.join(' · ')
}

function compactTokensText(e: Extract<StreamEntry, { kind: 'compact' }>): string {
  if (!e.preTokens) return ''
  const tokens = e.postTokens ? `${formatTokens(e.preTokens)} → ${formatTokens(e.postTokens)}` : formatTokens(e.preTokens)
  return t('streamLog.tokensSuffix', { tokens })
}

// --- Entry height measurement (feeds `content-visibility`) ------------------
// An entry may only be skipped off-screen once we know how tall it really is;
// see the `.stream-entry[data-measured]` rule. One shared ResizeObserver across
// every StreamLog instance measures each entry while it is rendered and records
// that height on the element itself, so the placeholder it leaves behind is the
// exact size it occupied — scrollHeight then stays put as entries scroll in and
// out, and re-reading a long run no longer jumps.
const measuredHeights = new WeakMap<Element, number>()

// Skipped entries report their placeholder height, not a new measurement —
// writing that back would slowly drift the reserved size. Re-measure only what
// is actually being rendered.
function isRendering(el: HTMLElement): boolean {
  if (!el.hasAttribute('data-measured')) return true
  if (typeof el.checkVisibility !== 'function') return true
  return el.checkVisibility({ contentVisibilityAuto: true } as CheckVisibilityOptions)
}

// Skipping is not free while scrolling: an entry that comes back into view has
// to be laid out and painted from scratch inside that frame. Scrolling past
// content that is already laid out costs nothing but a paint — so an entry only
// earns its keep if it is big enough that NOT holding it laid out is a real
// saving. Small entries (the one-line tool headers that make up most of a run)
// stay rendered, which is what keeps scrolling smooth; the rare huge entry (a
// long diff, a wall of markdown) is still skipped, and that's where the layout
// cost of a long run actually lives.
const SKIP_MIN_HEIGHT = 1200

// Measurements are applied on the next frame, not inside the observer callback:
// toggling `data-measured` changes the entry's layout (content-visibility), and
// doing that within the callback makes the browser defer the remaining
// notifications and log "ResizeObserver loop completed with undelivered
// notifications".
const pendingHeights = new Map<HTMLElement, number>()
let applyFrame = 0

function applyPendingHeights() {
  applyFrame = 0
  for (const [el, h] of pendingHeights) {
    if (!el.isConnected) continue
    if (h < SKIP_MIN_HEIGHT) {
      // Shrank below the threshold (a tool card collapsed, say): stop
      // skipping it rather than leaving a stale reservation behind.
      el.removeAttribute('data-measured')
      continue
    }
    el.style.setProperty('--stream-entry-h', `${Math.round(h)}px`)
    el.setAttribute('data-measured', '')
  }
  pendingHeights.clear()
}

const entrySizeObserver =
  typeof ResizeObserver === 'undefined'
    ? null
    : new ResizeObserver((records) => {
        for (const rec of records) {
          const el = rec.target as HTMLElement
          const h = rec.borderBoxSize?.[0]?.blockSize ?? rec.contentRect.height
          if (h <= 0) continue
          const prev = measuredHeights.get(el)
          if (prev !== undefined && Math.abs(prev - h) < 1) continue
          if (!isRendering(el)) continue
          measuredHeights.set(el, h)
          pendingHeights.set(el, h)
        }
        if (pendingHeights.size && !applyFrame) applyFrame = requestAnimationFrame(applyPendingHeights)
      })

// Entries mounted by THIS log, so a width change can invalidate their heights.
const observedEntries = new Set<HTMLElement>()

const vEntrySize = {
  mounted: (el: HTMLElement) => {
    observedEntries.add(el)
    entrySizeObserver?.observe(el)
  },
  unmounted: (el: HTMLElement) => {
    observedEntries.delete(el)
    pendingHeights.delete(el)
    entrySizeObserver?.unobserve(el)
  },
}

// A narrower column re-wraps text, so every measurement taken at the old width
// is stale — but a skipped entry is never laid out again, so it would keep
// reserving its old height and resize the moment it scrolls back into view.
// Drop the measurements on a width change: each entry renders normally until it
// has been measured again. Debounced, so dragging the split doesn't re-measure
// on every intermediate pixel.
const rootEl = ref<HTMLElement | null>(null)
let lastWidth = 0
let invalidateTimer: ReturnType<typeof setTimeout> | undefined
let widthObserver: ResizeObserver | null = null

function invalidateMeasurements() {
  pendingHeights.clear()
  for (const el of observedEntries) {
    el.removeAttribute('data-measured')
    measuredHeights.delete(el)
  }
}

onMounted(() => {
  const el = rootEl.value
  if (!el || typeof ResizeObserver === 'undefined') return
  lastWidth = el.clientWidth
  widthObserver = new ResizeObserver(() => {
    const w = rootEl.value?.clientWidth ?? 0
    if (!w || Math.abs(w - lastWidth) < 1) return
    lastWidth = w
    clearTimeout(invalidateTimer)
    invalidateTimer = setTimeout(invalidateMeasurements, 150)
  })
  widthObserver.observe(el)
})

// ── Quote cards on a sent prompt (UX-29) ─────────────────────────────────────
// The prompt's text starts with its quotes as a blockquote (that is what the
// model got); with the cards shown, the bubble drops that part.
function quoteSetOf(e: StreamEntry): QuoteSetView | null {
  return e.kind === 'user' && e.quoteSet ? props.quoteSets?.get(e.quoteSet) ?? null : null
}
function bubbleText(e: Extract<StreamEntry, { kind: 'user' }>): string {
  const set = quoteSetOf(e)
  return set ? stripQuotePrefix(e.text, set.prefix) ?? e.text : e.text
}
function quoteSourceLabel(q: RunQuote): string {
  if (q.kind !== 'file' || !q.path) return ''
  if (q.line_start == null) return q.path
  return q.line_end != null && q.line_end > q.line_start ? `${q.path}:${q.line_start}-${q.line_end}` : `${q.path}:${q.line_start}`
}
function openQuote(card: QuoteCard) {
  if (!card.jumpable) return
  const q = card.quote
  if (q.kind === 'file' && q.path) emit('open-file', q.path, q.line_start)
  else emit('quote-jump', q)
}

// ── Typewriter smoothing ────────────────────────────────────────────────────
// Partial deltas land in bursts (a frame's flush can carry several), so the text
// entry being streamed reveals its text at an adaptive pace instead of in
// chunks: each frame shows a share of what is still hidden, so it never trails
// far behind. Settled entries always render in full.
const streamingTextIndex = computed(() => {
  const list = props.entries
  for (let i = list.length - 1; i >= Math.max(0, list.length - 50); i--) {
    const e = list[i]
    if (e.kind === 'text' && e.streaming) return i
  }
  return -1
})
const streamingTextLength = computed(() => {
  const e = props.entries[streamingTextIndex.value]
  return e?.kind === 'text' ? e.text.length : 0
})
const shownLength = ref(0)
const reduceMotion = typeof window !== 'undefined'
  && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
let smoothFrame = 0

function revealTick() {
  smoothFrame = 0
  const hidden = streamingTextLength.value - shownLength.value
  if (hidden <= 0) return
  shownLength.value += Math.max(2, Math.ceil(hidden / 8))
  if (shownLength.value < streamingTextLength.value) smoothFrame = requestAnimationFrame(revealTick)
}

watch(streamingTextIndex, () => { shownLength.value = 0 })
watch(streamingTextLength, (len) => {
  if (reduceMotion || len < shownLength.value) shownLength.value = len
  else if (!smoothFrame) smoothFrame = requestAnimationFrame(revealTick)
})

function displayText(e: { text: string }, i: number): string {
  return i === streamingTextIndex.value ? e.text.slice(0, shownLength.value) : e.text
}

onBeforeUnmount(() => {
  cancelAnimationFrame(smoothFrame)
  widthObserver?.disconnect()
  widthObserver = null
  clearTimeout(invalidateTimer)
  for (const el of observedEntries) entrySizeObserver?.unobserve(el)
  observedEntries.clear()
})

// ── Row of buttons under each turn (Copy · Quote · Fullscreen · …) ──────────
// Only where a selection host exists (the desktop app, not the controller web
// client). The selection surface declared around this log (RunView's output)
// decides where a quote and a saved note go — the same place as for a
// selection in it.
const owner = computed(() => {
  const el = rootEl.value
  return el ? pickSurface(selectionSurfaces, surfaceEls, el, el) : null
})
const quoteTarget = computed(() => (owner.value?.allow.quote ? owner.value.quoteTarget?.() ?? null : null))
const turnActionsOn = computed(() => !!selectionHost.value && !props.noTurnActions)

// Recomputed when an entry is added, not per streamed chunk: the running turn,
// whose text is still growing, is never read here.
const footers = computed(() =>
  turnActionsOn.value ? turnFooters(props.entries, !!props.running) : new Map<number, TurnFooter>(),
)

// v-memo dependency: the row under entry `i` and what it shows.
function footerKey(i: number): string {
  const f = footers.value.get(i)
  return f ? `${f.side}:${f.from}:${f.to}:${f.hasText ? 1 : 0}:${f.rewindTurn ?? ''}` : ''
}

const msgActionsState: MsgActionsState = {
  isOn: (id) => selectionHost.value?.isOn(id) ?? false,
  quoteLabel: computed(() => quoteTarget.value?.label ?? null),
  canSaveNote: computed(() => !!owner.value?.allow.saveNote && !!owner.value.capture),
  canRewind: (turn) => owner.value?.rewind?.()?.available(turn) ?? false,
  canFork: () => !!owner.value?.fork?.(),
  canEditResend: (turn) => owner.value?.editResend?.()?.available(turn) ?? false,
  bookmarked: (promptAt, side) => owner.value?.bookmarks?.()?.marked(promptAt, side) ?? null,
}

interface Fullscreen {
  title: string
  /** The message, as copied: raw text of a question, markdown of an answer. */
  text: string
  /** Question: shown as typed, like its bubble. */
  plain?: boolean
  /** Whole turn: the answer's entries, tool calls included. */
  entries?: StreamEntry[]
}
const fullscreen = ref<Fullscreen | null>(null)

// The Quote under an answer points at the answer's first message: the whole
// message (start = end), as the quote is the whole answer.
function answerAnchor(f: TurnFooter): QuoteAnchor | null {
  for (let k = f.from; k <= f.to && k < props.entries.length; k++) {
    const e = props.entries[k]
    if (e.kind === 'text' && e.mid) return { entryId: e.mid, block: e.block ?? 0, start: 0, end: 0 }
  }
  return null
}

async function runTurnAction(i: number, action: TurnAction, from: DOMRect): Promise<boolean> {
  const f = footers.value.get(i)
  if (!f) return false
  const e = props.entries[i]
  const question = f.side === 'question'
  const text = question ? (e.kind === 'user' ? e.text : '') : turnAnswerText(props.entries, f.from, f.to)
  switch (action) {
    case 'copy':
      try {
        await navigator.clipboard.writeText(text)
        return true
      } catch {
        return false // clipboard denied
      }
    case 'quote':
      // Through the Quote popover (SelectionToolbar), opened at the button, for a note first.
      if (owner.value && quoteTarget.value) {
        selectionQuoteNote.value = {
          surface: owner.value,
          target: quoteTarget.value,
          text,
          anchor: question ? null : answerAnchor(f),
          at: { left: from.left, top: from.top, bottom: from.bottom },
        }
      }
      break
    case 'fullscreen':
      fullscreen.value = { title: t(question ? 'streamLog.turn.questionTitle' : 'streamLog.turn.answerTitle'), text, plain: question }
      break
    case 'fullscreenTurn':
      fullscreen.value = { title: t('streamLog.turn.turnTitle'), text, entries: props.entries.slice(f.from, f.to + 1) }
      break
    case 'saveNote':
      if (owner.value) selectionHost.value?.saveNote(owner.value, text)
      break
    case 'rewind':
      if (f.rewindTurn !== null) owner.value?.rewind?.()?.start(f.rewindTurn)
      break
    case 'fork':
      owner.value?.fork?.()?.start(turnPromptAt(f))
      break
    case 'editResend':
      // The prompt as sent (quotes as blockquotes first) and its saved quotes:
      // the composer splits them back into text and cards.
      if (f.rewindTurn !== null && e.kind === 'user') {
        owner.value?.editResend?.()?.start(f.rewindTurn, i, {
          text: e.text,
          quotes: quoteSetOf(e)?.cards.map((c) => c.quote) ?? [],
          images: e.images,
        })
      }
      break
    case 'bookmark':
      owner.value?.bookmarks?.()?.toggle(turnPromptAt(f), f.side)
      break
  }
  return false
}

// ── Folded turn work (tool calls · thinking · commentary) ─────────────────────
// lib/turnActivity decides which entries fold; the header sits above a block's
// first entry and folded entries are simply not rendered, so every other entry
// (prompts with their `data-user-turn` landmark, answers, the rows of buttons)
// keeps its place in this flat list. Like the rows, it reads no growing text,
// so a streamed chunk does not recompute it.
const NO_ACTIVITY: TurnActivityMap = { blocks: [], memberOf: new Map(), headerAt: new Map() }
const activity = computed(() =>
  props.groupSteps ? turnActivity(props.entries, !!props.running) : NO_ACTIVITY,
)

// The reader's open / closed choice per block, keyed by the block's turn prompt
// (by identity, not index: older history prepended shifts every index).
const blockIds = new WeakMap<object, number>()
let nextBlockId = 0
function blockId(b: TurnActivity): number {
  const key = toRaw(props.entries[b.keyAt]) as object
  let id = blockIds.get(key)
  if (id === undefined) {
    id = nextBlockId++
    blockIds.set(key, id)
  }
  return id
}
const blockChoice = ref<Record<number, boolean>>({})
// Flipping "expand all" is a fresh start: it overrides every per-block choice.
watch(() => props.stepsExpanded, () => { blockChoice.value = {} })

// Blocks whose visible text holds the ⌘F query: they open so the in-page
// highlight (which searches the DOM) can reach the match.
const findHits = computed(() => {
  const hits = new Set<number>()
  const q = props.findQuery
  if (!q) return hits
  activity.value.blocks.forEach((b, bi) => {
    if (activityMatches(props.entries, b, q, !!props.findCaseSensitive)) hits.add(bi)
  })
  return hits
})
// A search beats an earlier "close": drop the choice on blocks it hits, so the
// reader can still close one again while the bar is open.
watch(findHits, (hits) => {
  const next = { ...blockChoice.value }
  let changed = false
  for (const bi of hits) {
    const id = blockId(activity.value.blocks[bi])
    if (id in next) {
      delete next[id]
      changed = true
    }
  }
  if (changed) blockChoice.value = next
})

const blockOpen = computed(() =>
  activity.value.blocks.map((b, bi) =>
    activityOpen(b, {
      choice: blockChoice.value[blockId(b)],
      findHit: findHits.value.has(bi),
      expandAll: props.stepsExpanded,
    }),
  ),
)

function toggleBlock(bi: number) {
  const b = activity.value.blocks[bi]
  if (b) blockChoice.value = { ...blockChoice.value, [blockId(b)]: !blockOpen.value[bi] }
}

// Jumping to a quote's source: open the block its message is folded in.
watch(() => props.quoteReveal, (r) => {
  if (!r) return
  const next = { ...blockChoice.value }
  let changed = false
  props.entries.forEach((e, i) => {
    if (e.kind !== 'text' || e.mid !== r.id) return
    const bi = activity.value.memberOf.get(i)
    if (bi === undefined || blockOpen.value[bi]) return
    next[blockId(activity.value.blocks[bi])] = true
    changed = true
  })
  if (changed) blockChoice.value = next
})

function isFolded(i: number): boolean {
  const bi = activity.value.memberOf.get(i)
  return bi !== undefined && !blockOpen.value[bi]
}

// How a shown entry sits in its block: on the rail, or on its last stretch
// (no rail tail below it). '' = not in a block. Also a v-memo dependency.
function stepClass(i: number): '' | 'turn-step' | 'turn-step turn-step-last' {
  const bi = activity.value.memberOf.get(i)
  if (bi === undefined) return ''
  const m = activity.value.blocks[bi].members
  return m[m.length - 1] === i ? 'turn-step turn-step-last' : 'turn-step'
}

// A tool result attaches to its entry AFTER the tool call is first rendered
// (same object, mutated in place). v-memo compares by reference, so we surface
// it as an explicit memo dependency — this is the one in-place mutation an
// otherwise-finalized entry undergoes.
function toolResult(e: StreamEntry): unknown {
  return e.kind === 'tool' ? e.result : undefined
}
</script>

<template>
  <div ref="rootEl" class="space-y-3">
    <template v-for="(entry, i) in entries" :key="i">
      <!-- v-memo: freeze a finalized entry so a streamed update to the LAST
           entry (or a tool result attaching, a toggle, a copy flash) never
           re-renders the whole history. Deps list every reactive value this
           entry's markup reads: the entry itself, its late-attached result,
           and the per-index toggle/flash/collapse state. -->
      <!-- `stream-entry` opts the entry into `content-visibility: auto` so WebKit
           skips layout+paint while it is off-screen, keeping the reflow cost of
           each streamed chunk proportional to what's on screen. It takes effect
           only once `v-entry-size` has measured the entry (so the skipped entry
           reserves the height it really had) and only WHILE THE RUN IS LIVE.
           Reading a finished run is pure scrolling: content that stays laid out
           costs a paint to scroll past, whereas a skipped entry has to be laid
           out again from scratch in the frame it re-enters the viewport — so
           skipping is a loss there, and that stutter is what the reader feels.
           The last entry is excluded while streaming: it's always in view and
           its height changes every chunk, which makes the placeholder thrash. -->
      <!-- Header of the turn's folded work, above its first step. Open, it
           hugs the rail the steps hang from. -->
      <div
        v-if="activity.headerAt.has(i)"
        :class="{ 'turn-activity-open': blockOpen[activity.headerAt.get(i)!] }"
      >
        <TurnActivityHeader
          :block="activity.blocks[activity.headerAt.get(i)!]"
          :open="blockOpen[activity.headerAt.get(i)!]"
          @toggle="toggleBlock(activity.headerAt.get(i)!)"
        />
      </div>
      <!-- `data-user-turn` marks the reader's landmarks: the turn navigator in
           RunView walks these to jump between the user's own messages. On the
           wrapper, not the bubble, so the jump lands on the top of the turn.
           Prompts never fold, so the landmarks are always there. -->
      <template v-if="!isFolded(i)">
      <div
        v-entry-size
        :data-user-turn="entry.kind === 'user' ? i : undefined"
        :class="{ 'stream-entry': running && i !== entries.length - 1, 'turn-step-wrap': stepClass(i) === 'turn-step' }"
        v-memo="[entry, toolResult(entry), expanded[i], expandedCompacts.has(i), copiedCmd[i], expandedMessageIndices.has(i), running && i === entries.length - 1, i === streamingTextIndex ? shownLength : -1, footerKey(i), stepClass(i), quoteSetOf(entry), fileListVersion]"
      >
      <div :class="stepClass(i)">
      <!-- System init banner -->
      <div
        v-if="entry.kind === 'system'"
        class="flex items-center gap-2 text-2xs font-mono text-foreground/40 border-b border-border pb-2"
      >
        <Cpu class="h-3 w-3" :stroke-width="1.5" />
        <span>{{ t('streamLog.session') }}</span>
        <span v-if="entry.model" class="text-foreground/70">{{ entry.model }}</span>
        <span v-if="entry.tools?.length" class="text-foreground/30">· {{ t('streamLog.tools', { count: entry.tools.length }) }}</span>
      </div>

      <!-- User follow-up — right-aligned chat bubble with avatar -->
      <div v-else-if="entry.kind === 'user'" class="flex justify-end items-end gap-2 pl-10 my-1">
        <div
          class="max-w-[82%] rounded-2xl rounded-br-md bg-muted/70 border border-border px-3.5 py-2 text-sm leading-relaxed text-foreground shadow-sm"
        >
          <div
            v-if="entry.ideContext?.length"
            class="mb-2 pb-2 border-b border-border flex flex-col items-end"
          >
            <IdeContextChip :items="entry.ideContext" variant="inside" @open-file="(p) => emit('open-file', p)" />
          </div>
          <!-- Its structured quotes (UX-29): numbered cards that lead back to the source. -->
          <ol v-if="quoteSetOf(entry)" class="mb-2 flex flex-col gap-1.5">
            <li v-for="(card, ci) in quoteSetOf(entry)!.cards" :key="card.quote.id">
              <button
                type="button"
                class="w-full flex items-start gap-2 rounded-lg border border-border bg-background/60 px-2.5 py-1.5 text-left text-xs transition-colors"
                :class="card.jumpable ? 'cursor-pointer hover:bg-accent/60 hover:border-primary/40' : 'cursor-default opacity-80'"
                :disabled="!card.jumpable"
                :title="card.jumpable ? (card.quote.kind === 'file' ? t('streamLog.quotes.openFile') : t('streamLog.quotes.goToSource')) : undefined"
                @click="openQuote(card)"
              >
                <span class="shrink-0 mt-px h-4 min-w-4 px-1 rounded-full bg-primary/15 text-primary text-2xs font-semibold flex items-center justify-center">{{ ci + 1 }}</span>
                <span class="min-w-0 flex-1">
                  <span class="block whitespace-pre-wrap text-foreground/80 line-clamp-3">{{ card.quote.excerpt }}</span>
                  <span v-if="card.quote.note" class="mt-0.5 block whitespace-pre-wrap text-foreground">{{ card.quote.note }}</span>
                  <span v-if="card.quote.kind === 'file'" class="mt-0.5 block truncate font-mono text-2xs text-muted-foreground">{{ quoteSourceLabel(card.quote) }}</span>
                  <span
                    v-else-if="card.quote.kind === 'entry' && !card.jumpable"
                    class="mt-0.5 block text-2xs italic text-muted-foreground"
                  >{{ t('streamLog.quotes.sourceGone') }}</span>
                </span>
              </button>
            </li>
          </ol>
          <div
            v-if="entry.images && entry.images.length"
            class="flex flex-wrap gap-1.5"
            :class="bubbleText(entry) ? 'mb-2' : ''"
          >
            <img
              v-for="(img, ii) in entry.images"
              :key="ii"
              :src="`data:${img.media_type};base64,${img.data}`"
              class="max-h-40 max-w-48 rounded-md border border-primary/20 object-contain"
              alt="attachment"
            />
          </div>
          <CollapsibleMessage v-if="bubbleText(entry)" :max-height="256" fade="hsl(var(--muted))" :disabled="expandedMessageIndices.has(i)">
            <div class="whitespace-pre-wrap"><template
                v-for="(seg, si) in userSegments(bubbleText(entry))"
                :key="si"
              ><span
                  v-if="seg.path"
                  class="font-medium text-primary cursor-pointer underline decoration-dotted underline-offset-2 hover:text-primary/75"
                  :title="t('streamLog.openFile', { path: seg.path })"
                  role="button"
                  @click="emit('open-file', seg.path!, seg.line)"
                >{{ seg.text }}</span><template v-else>{{ seg.text }}</template></template></div>
          </CollapsibleMessage>
        </div>
        <div
          class="shrink-0 h-7 w-7 rounded-full bg-muted border border-border flex items-center justify-center"
          :title="t('streamLog.you')"
        >
          <User class="h-3.5 w-3.5 text-primary/70" :stroke-width="2" />
        </div>
      </div>

      <!-- IDE context — file the editor auto-attached to the turn (opened file
           or selection). A subtle chip, not a chat bubble: the user didn't type
           it. The filename opens the file in the viewer. -->
      <div v-else-if="entry.kind === 'ide-context'" class="flex justify-end pl-10 -my-0.5">
        <IdeContextChip :items="entry.items" variant="standalone" @open-file="(p) => emit('open-file', p)" />
      </div>

      <!-- Compaction summary — context was condensed; show a divider, expandable to read it -->
      <div v-else-if="entry.kind === 'compact'" class="my-3">
        <div class="flex items-center gap-2 text-foreground/40">
          <span class="h-px flex-1 bg-border" />
          <button
            class="inline-flex items-center gap-1.5 text-2xs font-medium uppercase tracking-wide hover:text-foreground/70 transition-colors"
            :title="entry.summary ? (expandedCompacts.has(i) ? t('streamLog.hideSummary') : t('streamLog.showSummary')) : undefined"
            @click="entry.summary && toggleCompact(i)"
          >
            <Archive class="h-3 w-3" :stroke-width="1.75" />
            {{ t('streamLog.contextCompacted') }}
            <span v-if="entry.preTokens" class="font-mono normal-case tracking-normal tabular-nums">{{ compactTokensText(entry) }}</span>
            <ChevronRight v-if="entry.summary" class="h-3 w-3 transition-transform" :class="expandedCompacts.has(i) ? 'rotate-90' : ''" :stroke-width="2" />
          </button>
          <span class="h-px flex-1 bg-border" />
        </div>
        <div
          v-if="expandedCompacts.has(i) && entry.summary"
          v-file-links
          v-mermaid
          v-copy-code
          v-html="renderText(entry.summary)"
          class="markdown-output mt-2 text-xs leading-relaxed text-foreground/70 bg-muted/40 border border-border rounded-lg px-3 py-2.5"
          @click="onProseClick"
        />
      </div>

      <!-- Local slash command (e.g. /compact) the user ran — a slim marker, not a chat bubble -->
      <div v-else-if="entry.kind === 'command'" class="my-1.5 flex flex-col items-center gap-1">
        <div
          v-if="entry.name"
          class="inline-flex items-center gap-1.5 rounded-full bg-muted/60 border border-border px-2.5 py-1 text-2xs"
        >
          <Terminal class="h-3 w-3 text-foreground/50" :stroke-width="1.75" />
          <span class="font-mono font-medium text-foreground/75">{{ entry.name }}</span>
          <span v-if="entry.args" class="font-mono text-foreground/55">{{ entry.args }}</span>
        </div>
        <div v-if="entry.stdout" class="text-2xs font-mono text-foreground/45">{{ entry.stdout }}</div>
      </div>

      <!-- Harness-injected message (skill body, conductor nudge, meta) — a slim
           marker the user can expand; it isn't something they typed. -->
      <div v-else-if="entry.kind === 'injected'" class="my-1.5 flex flex-col items-start gap-1">
        <button
          class="inline-flex items-center gap-1.5 rounded-full bg-muted/60 border border-border px-2.5 py-1 text-2xs text-foreground/60 hover:text-foreground/80 transition-colors"
          :title="expandedCompacts.has(i) ? t('streamLog.hideInjected') : t('streamLog.showInjected')"
          @click="toggleCompact(i)"
        >
          <BookOpen v-if="entry.source === 'skill'" class="h-3 w-3" :stroke-width="1.75" />
          <BellRing v-else-if="entry.source === 'session'" class="h-3 w-3" :stroke-width="1.75" />
          <Info v-else class="h-3 w-3" :stroke-width="1.75" />
          <span>{{ t(`streamLog.injected.${entry.source}`) }}</span>
          <span v-if="entry.label" class="font-mono font-medium text-foreground/75">{{ entry.label }}</span>
          <ChevronRight class="h-3 w-3 transition-transform" :class="expandedCompacts.has(i) ? 'rotate-90' : ''" :stroke-width="2" />
        </button>
        <div
          v-if="expandedCompacts.has(i)"
          v-file-links
          v-mermaid
          v-copy-code
          v-html="renderText(entry.text)"
          class="markdown-output w-full mt-1 text-xs leading-relaxed text-foreground/70 bg-muted/40 border border-border rounded-lg px-3 py-2.5"
          @click="onProseClick"
        />
      </div>

      <!-- Assistant text — always rendered in full, never collapsed.
           renderText's 2nd arg skips the markdown memo while this entry is the
           one being streamed: its text grows every chunk, so each render is a
           new cache key that will never be read again and only serves to evict
           the finalized entries that DO get re-read on scroll. -->
      <!-- `data-quote-*`: the message a quote taken from here is anchored to (lib/quoteAnchor). -->
      <div v-else-if="entry.kind === 'text'">
        <div
          v-file-links
          v-mermaid
          v-copy-code
          v-bind="entry.mid ? { [QUOTE_SRC_ATTR]: entry.mid, [QUOTE_BLOCK_ATTR]: entry.block ?? 0 } : {}"
          v-html="renderText(displayText(entry, i), (running && i === entries.length - 1) || !!entry.streaming)"
          class="markdown-output text-sm leading-relaxed text-foreground"
          @click="onProseClick"
        />
      </div>

      <!-- Thinking -->
      <div
        v-else-if="entry.kind === 'thinking'"
        class="text-xs italic text-foreground/40 border-l-2 border-foreground/10 pl-3 py-1"
      >
        <div class="flex items-center gap-1 mb-1 text-2xs uppercase tracking-wider not-italic">
          <Brain class="h-3 w-3" :stroke-width="1.5" />
          {{ t('streamLog.thinking') }}
        </div>
        <div class="whitespace-pre-wrap">{{ entry.text }}</div>
      </div>

      <!-- Bash-family tool call — terminal-style card: purpose headline, the real
           command in a copyable terminal block, then its output. -->
      <div
        v-else-if="entry.kind === 'tool' && isBash(entry.name)"
        class="rounded-lg border border-l-[3px] bg-card/80 shadow-sm shadow-black/20 overflow-hidden"
        :class="entry.result?.is_error
          ? 'border-border border-l-destructive/70'
          : ['border-border', toolStyle(entry.name).barClass]"
      >
        <button
          class="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-foreground/[0.07] transition-colors cursor-pointer"
          @click="toggle(i)"
        >
          <ChevronRight
            class="h-3 w-3 text-foreground/40 transition-transform shrink-0"
            :stroke-width="1.75"
            :class="{ 'rotate-90': expanded[i] }"
          />
          <Terminal class="h-3.5 w-3.5 shrink-0 text-emerald-400" :stroke-width="1.75" />
          <!-- Primary line: the command's purpose (description), or the command
               itself when no description was provided. -->
          <span class="text-xs text-foreground/80 truncate">{{ bashHeadline(entry.name, entry.input) }}</span>
          <span
            v-if="bashInfo(entry.input).background"
            class="text-2xs uppercase tracking-wider text-foreground/40 border border-border rounded px-1 shrink-0"
          >bg</span>
          <span v-if="entry.result?.is_error" class="ml-auto text-2xs text-destructive shrink-0 flex items-center gap-0.5">
            <XCircle class="h-3 w-3" :stroke-width="1.75" />{{ t('streamLog.error') }}
          </span>
          <span v-else-if="entry.result" class="ml-auto text-2xs text-success/80 shrink-0 flex items-center gap-0.5">
            <CheckCircle2 class="h-3 w-3" :stroke-width="1.75" />
          </span>
          <span v-else class="ml-auto text-2xs text-warning/80 animate-pulse shrink-0">{{ runningText(entry, t('streamLog.running')) }}</span>
        </button>
        <div v-if="expanded[i]" class="border-t border-border">
          <!-- Real command executed on the machine — terminal look + copy.
               Shell-management tools (BashOutput/KillShell) carry no command,
               so skip the block entirely rather than render an empty `$`. -->
          <div v-if="bashInfo(entry.input).command" class="px-3 py-2 bg-zinc-950/90 dark:bg-black/40 relative group/cmd">
            <div class="text-2xs uppercase tracking-wider text-white/40 mb-1 flex items-center justify-between">
              <span>{{ t('streamLog.command') }}</span>
              <button
                class="inline-flex items-center gap-1 text-white/60 hover:text-white cursor-pointer"
                :title="copiedCmd[i] ? t('streamLog.copied') : t('streamLog.copyCommand')"
                @click.stop="copyCommand(i, entry.input)"
              >
                <component :is="copiedCmd[i] ? Check : Copy" class="h-3 w-3" :stroke-width="1.75" />
                <span class="text-2xs">{{ copiedCmd[i] ? t('streamLog.copiedShort') : t('streamLog.copyShort') }}</span>
              </button>
            </div>
            <pre class="text-2xs font-mono text-emerald-300/90 whitespace-pre-wrap break-words leading-relaxed"><span class="select-none text-emerald-500/60">$ </span>{{ bashInfo(entry.input).command }}</pre>
          </div>
          <!-- Command output (stdout/stderr). -->
          <div v-if="entry.result" class="px-3 py-2 border-t border-border">
            <div class="text-2xs uppercase tracking-wider text-foreground/40 mb-1">
              {{ entry.result.is_error ? t('streamLog.error') : t('streamLog.output') }}
            </div>
            <pre
              v-if="entry.result.content"
              class="text-2xs font-mono whitespace-pre-wrap break-words max-h-96 overflow-auto"
              :class="entry.result.is_error ? 'text-destructive' : 'text-foreground/70'"
            >{{ entry.result.content }}</pre>
            <div v-else class="text-2xs font-mono text-foreground/30 italic">{{ t('streamLog.noOutput') }}</div>
          </div>
        </div>
      </div>

      <!-- File-editing tool call (Edit / MultiEdit / Write) — unified diff view. -->
      <div
        v-else-if="entry.kind === 'tool' && isEditTool(entry.name)"
        class="rounded-lg border border-l-[3px] bg-card/80 shadow-sm shadow-black/20 overflow-hidden"
        :class="entry.result?.is_error
          ? 'border-border border-l-destructive/70'
          : ['border-border', toolStyle(entry.name).barClass]"
      >
        <button
          class="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-foreground/[0.07] transition-colors cursor-pointer"
          @click="toggle(i)"
        >
          <ChevronRight
            class="h-3 w-3 text-foreground/40 transition-transform shrink-0"
            :stroke-width="1.75"
            :class="{ 'rotate-90': expanded[i] }"
          />
          <component
            :is="toolStyle(entry.name).icon"
            class="h-3.5 w-3.5 shrink-0"
            :class="toolStyle(entry.name).iconClass"
            :stroke-width="1.75"
          />
          <span class="text-xs font-mono font-semibold shrink-0" :class="toolStyle(entry.name).nameClass">{{ entry.name }}</span>
          <span
            v-if="fileTarget(entry.input)"
            class="text-2xs font-mono text-info/90 truncate underline decoration-dotted underline-offset-2 hover:text-info cursor-pointer"
            :title="t('streamLog.openFile', { path: fileTarget(entry.input) })"
            role="button"
            @click.stop="emit('open-file', fileTarget(entry.input)!)"
          >{{ fileTarget(entry.input) }}</span>
          <!-- One Codex patch can span several files; the rest are listed below. -->
          <span
            v-if="extraFileCount(entry.input)"
            class="text-2xs font-mono text-foreground/45 shrink-0"
            :title="toolFileTargets(entry.input).join('\n')"
          >{{ t('streamLog.moreFiles', { count: extraFileCount(entry.input) }) }}</span>
          <!-- +added / −removed stat badge -->
          <span
            v-if="editStats(entry.name, entry.input).added || editStats(entry.name, entry.input).removed"
            class="text-2xs font-mono shrink-0 flex items-center gap-1"
          >
            <span v-if="editStats(entry.name, entry.input).added" class="text-emerald-600 dark:text-emerald-400/80">+{{ editStats(entry.name, entry.input).added }}</span>
            <span v-if="editStats(entry.name, entry.input).removed" class="text-red-600 dark:text-red-400/80">−{{ editStats(entry.name, entry.input).removed }}</span>
          </span>
          <span v-if="entry.result?.is_error" class="ml-auto text-2xs text-destructive shrink-0 flex items-center gap-0.5">
            <XCircle class="h-3 w-3" :stroke-width="1.75" />{{ t('streamLog.error') }}
          </span>
          <span v-else-if="entry.result" class="ml-auto text-2xs text-success/80 shrink-0 flex items-center gap-0.5">
            <CheckCircle2 class="h-3 w-3" :stroke-width="1.75" />
          </span>
          <span v-else class="ml-auto text-2xs text-warning/80 animate-pulse shrink-0">{{ runningText(entry, t('streamLog.running')) }}</span>
        </button>
        <div v-if="expanded[i]" class="border-t border-border p-2 space-y-2">
          <template v-for="(part, pi) in editParts(entry.name, entry.input)" :key="pi">
            <!-- Per-file heading, shown once per file when a call spans several.
                 A single-file call already names it in the header. -->
            <div
              v-if="part.path && extraFileCount(entry.input) && part.path !== editParts(entry.name, entry.input)[pi - 1]?.path"
              class="px-1 pt-1 text-2xs font-mono text-info/90 truncate underline decoration-dotted underline-offset-2 hover:text-info cursor-pointer"
              :title="t('streamLog.openFile', { path: part.path })"
              role="button"
              @click="emit('open-file', part.path!)"
            >{{ part.path }}</div>
            <DiffView :before="part.before" :after="part.after" />
          </template>
          <div v-if="entry.result?.is_error" class="px-1">
            <div class="text-2xs uppercase tracking-wider text-foreground/40 mb-1">{{ t('streamLog.error') }}</div>
            <pre class="text-2xs font-mono text-destructive whitespace-pre-wrap break-words max-h-60 overflow-auto">{{ entry.result.content }}</pre>
          </div>
        </div>
      </div>

      <!-- Sub-agent (Task): subagent + goal in header, its report rendered as markdown. -->
      <div
        v-else-if="entry.kind === 'tool' && isAgentTool(entry.name)"
        class="rounded-lg border border-l-[3px] bg-card/80 shadow-sm shadow-black/20 overflow-hidden"
        :class="entry.result?.is_error ? 'border-border border-l-destructive/70' : ['border-border', toolStyle(entry.name).barClass]"
      >
        <button class="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-foreground/[0.07] transition-colors cursor-pointer" @click="toggle(i)">
          <ChevronRight class="h-3 w-3 text-foreground/40 transition-transform shrink-0" :stroke-width="1.75" :class="{ 'rotate-90': expanded[i] }" />
          <Bot class="h-3.5 w-3.5 shrink-0 text-fuchsia-400" :stroke-width="1.75" />
          <span class="text-xs font-mono font-semibold shrink-0 text-fuchsia-600 dark:text-fuchsia-300">{{ agentInfo(entry.input).subagent || t('streamLog.agent') }}</span>
          <span class="text-2xs text-foreground/60 truncate">{{ agentInfo(entry.input).description }}</span>
          <span
            v-if="entry.task && taskProgressText(entry.task)"
            class="text-2xs font-mono tabular-nums text-foreground/45 shrink-0"
            :class="{ 'animate-pulse': !entry.task.done }"
            :title="entry.task.summary"
          >{{ taskProgressText(entry.task) }}</span>
          <span v-if="entry.result?.is_error" class="ml-auto text-2xs text-destructive shrink-0 flex items-center gap-0.5"><XCircle class="h-3 w-3" :stroke-width="1.75" />{{ t('streamLog.error') }}</span>
          <span v-else-if="entry.result" class="ml-auto text-2xs text-success/80 shrink-0 flex items-center gap-0.5"><CheckCircle2 class="h-3 w-3" :stroke-width="1.75" /></span>
          <span v-else class="ml-auto text-2xs text-warning/80 animate-pulse shrink-0">{{ runningText(entry, t('streamLog.running')) }}</span>
        </button>
        <div v-if="expanded[i]" class="border-t border-border">
          <div v-if="agentInfo(entry.input).prompt" class="px-3 py-2 bg-black/20 dark:bg-black/25">
            <div class="text-2xs uppercase tracking-wider text-foreground/40 mb-1">{{ t('streamLog.task') }}</div>
            <pre class="text-2xs font-mono text-foreground/70 whitespace-pre-wrap break-words max-h-60 overflow-auto">{{ agentInfo(entry.input).prompt }}</pre>
          </div>
          <div v-if="entry.result" class="px-3 py-2 border-t border-border">
            <div class="text-2xs uppercase tracking-wider text-foreground/40 mb-1">{{ entry.result.is_error ? t('streamLog.error') : t('streamLog.result') }}</div>
            <div
              v-if="!entry.result.is_error"
              v-file-links v-mermaid v-copy-code
              v-html="renderText(entry.result.content)"
              class="markdown-output text-sm leading-relaxed text-foreground"
              @click="onProseClick"
            />
            <pre v-else class="text-2xs font-mono text-destructive whitespace-pre-wrap break-words max-h-96 overflow-auto">{{ entry.result.content }}</pre>
          </div>
        </div>
      </div>

      <!-- AskUserQuestion: questions + options, and the user's answer. -->
      <div
        v-else-if="entry.kind === 'tool' && isAskUserTool(entry.name)"
        class="rounded-lg border border-l-[3px] bg-card/80 shadow-sm shadow-black/20 overflow-hidden"
        :class="entry.result?.is_error ? 'border-border border-l-destructive/70' : ['border-border', toolStyle(entry.name).barClass]"
      >
        <button class="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-foreground/[0.07] transition-colors cursor-pointer" @click="toggle(i)">
          <ChevronRight class="h-3 w-3 text-foreground/40 transition-transform shrink-0" :stroke-width="1.75" :class="{ 'rotate-90': expanded[i] }" />
          <MessageCircleQuestion class="h-3.5 w-3.5 shrink-0 text-blue-400" :stroke-width="1.75" />
          <span class="text-xs font-mono font-semibold shrink-0 text-blue-600 dark:text-blue-300">{{ t('streamLog.question') }}</span>
          <span class="text-2xs text-foreground/60 truncate">{{ askQuestions(entry.input)[0]?.question }}</span>
          <span v-if="entry.result" class="ml-auto text-2xs text-success/80 shrink-0 flex items-center gap-0.5"><CheckCircle2 class="h-3 w-3" :stroke-width="1.75" /></span>
          <span v-else class="ml-auto text-2xs text-warning/80 animate-pulse shrink-0">{{ t('streamLog.waiting') }}</span>
        </button>
        <div v-if="expanded[i]" class="border-t border-border px-3 py-2 space-y-3">
          <div v-for="(q, qi) in askQuestions(entry.input)" :key="qi" class="space-y-1.5">
            <div class="text-xs font-medium text-foreground/85">{{ q.question }}</div>
            <div v-for="(opt, oi) in q.options" :key="oi" class="flex gap-2 text-2xs pl-2">
              <Circle class="h-3 w-3 mt-0.5 shrink-0 text-foreground/30" :stroke-width="1.75" />
              <div><span class="font-medium text-foreground/80">{{ opt.label }}</span><span v-if="opt.description" class="text-foreground/45"> — {{ opt.description }}</span></div>
            </div>
          </div>
          <div v-if="entry.result" class="pt-1 border-t border-border">
            <div class="text-2xs uppercase tracking-wider text-foreground/40 mb-1 mt-2">{{ t('streamLog.answer') }}</div>
            <pre class="text-2xs font-mono text-success whitespace-pre-wrap break-words">{{ entry.result.content }}</pre>
          </div>
        </div>
      </div>

      <!-- TodoWrite: render the plan as a status checklist. -->
      <div
        v-else-if="entry.kind === 'tool' && isTodoTool(entry.name)"
        class="rounded-lg border border-l-[3px] bg-card/80 shadow-sm shadow-black/20 overflow-hidden"
        :class="['border-border', toolStyle(entry.name).barClass]"
      >
        <button class="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-foreground/[0.07] transition-colors cursor-pointer" @click="toggle(i)">
          <ChevronRight class="h-3 w-3 text-foreground/40 transition-transform shrink-0" :stroke-width="1.75" :class="{ 'rotate-90': expanded[i] }" />
          <ListTodo class="h-3.5 w-3.5 shrink-0 text-blue-400" :stroke-width="1.75" />
          <span class="text-xs font-mono font-semibold shrink-0 text-blue-600 dark:text-blue-300">{{ t('streamLog.todos') }}</span>
          <span class="text-2xs text-foreground/50 tabular-nums shrink-0">{{ todoStats(entry.input).done }}/{{ todoStats(entry.input).total }}</span>
          <span class="text-2xs text-foreground/45 truncate">{{ todoItems(entry.input).find((t) => t.status === 'in_progress')?.content }}</span>
        </button>
        <div v-if="expanded[i]" class="border-t border-border px-3 py-2 space-y-1">
          <div v-for="(t, ti) in todoItems(entry.input)" :key="ti" class="flex items-start gap-2 text-xs">
            <CheckCircle2 v-if="t.status === 'completed'" class="h-3.5 w-3.5 mt-0.5 shrink-0 text-success" :stroke-width="1.75" />
            <CircleDot v-else-if="t.status === 'in_progress'" class="h-3.5 w-3.5 mt-0.5 shrink-0 text-warning animate-pulse" :stroke-width="1.75" />
            <Circle v-else class="h-3.5 w-3.5 mt-0.5 shrink-0 text-foreground/30" :stroke-width="1.75" />
            <span :class="{
              'line-through text-foreground/40': t.status === 'completed',
              'text-foreground/85 font-medium': t.status === 'in_progress',
              'text-foreground/70': t.status === 'pending',
            }">{{ t.content }}</span>
          </div>
        </div>
      </div>

      <!-- ToolSearch: the search query + matched tools. -->
      <div
        v-else-if="entry.kind === 'tool' && isToolSearchTool(entry.name)"
        class="rounded-lg border border-l-[3px] bg-card/80 shadow-sm shadow-black/20 overflow-hidden"
        :class="entry.result?.is_error ? 'border-border border-l-destructive/70' : ['border-border', toolStyle(entry.name).barClass]"
      >
        <button class="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-foreground/[0.07] transition-colors cursor-pointer" @click="toggle(i)">
          <ChevronRight class="h-3 w-3 text-foreground/40 transition-transform shrink-0" :stroke-width="1.75" :class="{ 'rotate-90': expanded[i] }" />
          <Search class="h-3.5 w-3.5 shrink-0 text-teal-400" :stroke-width="1.75" />
          <span class="text-xs font-mono font-semibold shrink-0 text-teal-600 dark:text-teal-300">{{ t('streamLog.toolSearch') }}</span>
          <span class="text-2xs font-mono text-foreground/50 truncate">{{ toolSearchQuery(entry.input) }}</span>
          <span v-if="entry.result?.is_error" class="ml-auto text-2xs text-destructive shrink-0 flex items-center gap-0.5"><XCircle class="h-3 w-3" :stroke-width="1.75" />{{ t('streamLog.error') }}</span>
          <span v-else-if="entry.result" class="ml-auto text-2xs text-success/80 shrink-0 flex items-center gap-0.5"><CheckCircle2 class="h-3 w-3" :stroke-width="1.75" /></span>
          <span v-else class="ml-auto text-2xs text-warning/80 animate-pulse shrink-0">{{ runningText(entry, t('streamLog.searching')) }}</span>
        </button>
        <div v-if="expanded[i]" class="border-t border-border">
          <div v-if="entry.result" class="px-3 py-2">
            <div class="text-2xs uppercase tracking-wider text-foreground/40 mb-1">{{ entry.result.is_error ? t('streamLog.error') : t('streamLog.matches') }}</div>
            <pre class="text-2xs font-mono whitespace-pre-wrap break-words max-h-96 overflow-auto" :class="entry.result.is_error ? 'text-destructive' : 'text-foreground/70'">{{ entry.result.content }}</pre>
          </div>
        </div>
      </div>

      <!-- WebFetch / WebSearch: fetched content rendered as markdown. -->
      <div
        v-else-if="entry.kind === 'tool' && isWebTool(entry.name)"
        class="rounded-lg border border-l-[3px] bg-card/80 shadow-sm shadow-black/20 overflow-hidden"
        :class="entry.result?.is_error ? 'border-border border-l-destructive/70' : ['border-border', toolStyle(entry.name).barClass]"
      >
        <button class="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-foreground/[0.07] transition-colors cursor-pointer" @click="toggle(i)">
          <ChevronRight class="h-3 w-3 text-foreground/40 transition-transform shrink-0" :stroke-width="1.75" :class="{ 'rotate-90': expanded[i] }" />
          <Globe class="h-3.5 w-3.5 shrink-0 text-cyan-400" :stroke-width="1.75" />
          <span class="text-xs font-mono font-semibold shrink-0 text-cyan-600 dark:text-cyan-300">{{ entry.name.toLowerCase() === 'websearch' ? t('streamLog.webSearch') : t('streamLog.webFetch') }}</span>
          <span class="text-2xs font-mono text-foreground/50 truncate">{{ previewInput(entry.input) }}</span>
          <span v-if="entry.result?.is_error" class="ml-auto text-2xs text-destructive shrink-0 flex items-center gap-0.5"><XCircle class="h-3 w-3" :stroke-width="1.75" />{{ t('streamLog.error') }}</span>
          <span v-else-if="entry.result" class="ml-auto text-2xs text-success/80 shrink-0 flex items-center gap-0.5"><CheckCircle2 class="h-3 w-3" :stroke-width="1.75" /></span>
          <span v-else class="ml-auto text-2xs text-warning/80 animate-pulse shrink-0">{{ runningText(entry, t('streamLog.loading')) }}</span>
        </button>
        <div v-if="expanded[i] && entry.result" class="border-t border-border px-3 py-2">
          <div
            v-if="!entry.result.is_error"
            v-file-links v-mermaid v-copy-code
            v-html="renderText(entry.result.content)"
            class="markdown-output text-sm leading-relaxed text-foreground max-h-[32rem] overflow-auto"
            @click="onProseClick"
          />
          <pre v-else class="text-2xs font-mono text-destructive whitespace-pre-wrap break-words max-h-96 overflow-auto">{{ entry.result.content }}</pre>
        </div>
      </div>

      <!-- ExitPlanMode: Claude's proposed plan, rendered as markdown. -->
      <div
        v-else-if="entry.kind === 'tool' && isPlanTool(entry.name)"
        class="rounded-lg border border-l-[3px] bg-card/80 shadow-sm shadow-black/20 overflow-hidden"
        :class="['border-border', toolStyle(entry.name).barClass]"
      >
        <button class="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-foreground/[0.07] transition-colors cursor-pointer" @click="toggle(i)">
          <ChevronRight class="h-3 w-3 text-foreground/40 transition-transform shrink-0" :stroke-width="1.75" :class="{ 'rotate-90': expanded[i] }" />
          <ClipboardList class="h-3.5 w-3.5 shrink-0 text-indigo-400" :stroke-width="1.75" />
          <span class="text-xs font-mono font-semibold shrink-0 text-indigo-600 dark:text-indigo-300">{{ t('streamLog.plan') }}</span>
          <span class="text-2xs text-foreground/45 truncate">{{ t('streamLog.proposedPlan') }}</span>
        </button>
        <div v-if="expanded[i]" class="border-t border-border px-3 py-2">
          <div
            v-file-links v-mermaid v-copy-code
            v-html="renderText(planContent(entry.input))"
            class="markdown-output text-sm leading-relaxed text-foreground"
            @click="onProseClick"
          />
        </div>
      </div>

      <!-- MCP tool / elicitation: server + action are surfaced up front. -->
      <div
        v-else-if="entry.kind === 'tool' && isMcpTool(entry.name)"
        class="rounded-lg border border-l-[3px] bg-card/80 shadow-sm shadow-black/20 overflow-hidden"
        :class="entry.result?.is_error ? 'border-border border-l-destructive/70' : ['border-border', toolStyle(entry.name).barClass]"
      >
        <button class="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-foreground/[0.07] transition-colors cursor-pointer" @click="toggle(i)">
          <ChevronRight class="h-3 w-3 text-foreground/40 transition-transform shrink-0" :stroke-width="1.75" :class="{ 'rotate-90': expanded[i] }" />
          <Server class="h-3.5 w-3.5 shrink-0 text-teal-400" :stroke-width="1.75" />
          <span class="text-xs font-mono font-semibold shrink-0 text-teal-600 dark:text-teal-300">{{ t('streamLog.mcp') }}</span>
          <span class="text-2xs font-mono shrink-0 rounded border border-border bg-foreground/4 px-1.5 py-0.5 text-foreground/55 truncate max-w-[10rem]" :title="mcpServerLabel(entry)">
            {{ mcpServerLabel(entry) }}
          </span>
          <span class="text-xs text-foreground/80 truncate">{{ mcpActionLabel(entry) }}</span>
          <span v-if="mcpSummary(entry)" class="text-2xs text-foreground/45 truncate">{{ mcpSummary(entry) }}</span>
          <span v-if="entry.result?.is_error" class="ml-auto text-2xs text-destructive shrink-0 flex items-center gap-0.5"><XCircle class="h-3 w-3" :stroke-width="1.75" />denied</span>
          <span v-else-if="entry.result" class="ml-auto text-2xs text-success/80 shrink-0 flex items-center gap-0.5"><CheckCircle2 class="h-3 w-3" :stroke-width="1.75" /></span>
          <span v-else class="ml-auto text-2xs text-warning/80 animate-pulse shrink-0">{{ runningText(entry, 'waiting…') }}</span>
        </button>
        <div v-if="expanded[i]" class="border-t border-border">
          <div class="grid grid-cols-[6rem_minmax(0,1fr)] gap-x-3 gap-y-1 px-3 py-2 bg-black/20 dark:bg-black/25 text-2xs">
            <div class="uppercase tracking-wider text-foreground/35">server</div>
            <div class="font-mono text-foreground/75 truncate">{{ mcpServerLabel(entry) }}</div>
            <div class="uppercase tracking-wider text-foreground/35">action</div>
            <div class="font-mono text-foreground/75 truncate">{{ mcpActionLabel(entry) }}</div>
            <template v-if="mcpSummary(entry)">
              <div class="uppercase tracking-wider text-foreground/35">message</div>
              <div class="text-foreground/70 whitespace-pre-wrap break-words">{{ mcpSummary(entry) }}</div>
            </template>
          </div>
          <div class="px-3 py-2 border-t border-border">
            <div class="text-2xs uppercase tracking-wider text-foreground/40 mb-1">input</div>
            <pre class="text-2xs font-mono text-foreground/70 whitespace-pre-wrap break-words max-h-60 overflow-auto">{{ prettyInput(entry.input) }}</pre>
          </div>
          <div v-if="entry.result" class="px-3 py-2 border-t border-border">
            <div class="text-2xs uppercase tracking-wider text-foreground/40 mb-1">{{ entry.result.is_error ? 'status' : 'output' }}</div>
            <pre
              class="text-2xs font-mono whitespace-pre-wrap break-words max-h-96 overflow-auto"
              :class="entry.result.is_error ? 'text-destructive' : 'text-foreground/70'"
            >{{ entry.result.content }}</pre>
          </div>
        </div>
      </div>

      <!-- Tool call -->
      <div
        v-else-if="entry.kind === 'tool'"
        class="rounded-lg border border-l-[3px] bg-card/80 shadow-sm shadow-black/20 overflow-hidden"
        :class="entry.result?.is_error
          ? 'border-border border-l-destructive/70'
          : ['border-border', toolStyle(entry.name).barClass]"
      >
        <button
          class="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-foreground/[0.07] transition-colors cursor-pointer"
          @click="toggle(i)"
        >
          <ChevronRight
            class="h-3 w-3 text-foreground/40 transition-transform shrink-0"
            :stroke-width="1.75"
            :class="{ 'rotate-90': expanded[i] }"
          />
          <component
            :is="toolStyle(entry.name).icon"
            class="h-3.5 w-3.5 shrink-0"
            :class="toolStyle(entry.name).iconClass"
            :stroke-width="1.75"
          />
          <span class="text-xs font-mono font-semibold shrink-0" :class="toolStyle(entry.name).nameClass">{{ entry.name }}</span>
          <!-- File-based tool: the path is clickable to preview the file. -->
          <span
            v-if="fileTarget(entry.input)"
            class="text-2xs font-mono text-info/90 truncate underline decoration-dotted underline-offset-2 hover:text-info cursor-pointer"
            :title="'Open ' + fileTarget(entry.input)"
            role="button"
            @click.stop="emit('open-file', fileTarget(entry.input)!)"
          >{{ fileTarget(entry.input) }}</span>
          <span v-else class="text-2xs font-mono text-foreground/40 truncate">{{ previewInput(entry.input) }}</span>
          <span v-if="entry.result?.is_error" class="ml-auto text-2xs text-destructive shrink-0 flex items-center gap-0.5">
            <XCircle class="h-3 w-3" :stroke-width="1.75" />error
          </span>
          <span v-else-if="entry.result" class="ml-auto text-2xs text-success/80 shrink-0 flex items-center gap-0.5">
            <CheckCircle2 class="h-3 w-3" :stroke-width="1.75" />
          </span>
          <span v-else class="ml-auto text-2xs text-warning/80 animate-pulse shrink-0">{{ runningText(entry, 'running…') }}</span>
        </button>
        <div v-if="expanded[i]" class="border-t border-border">
          <div class="px-3 py-2 bg-black/20 dark:bg-black/25">
            <div class="text-2xs uppercase tracking-wider text-foreground/40 mb-1">input</div>
            <pre class="text-2xs font-mono text-foreground/70 whitespace-pre-wrap break-words">{{ prettyInput(entry.input) }}</pre>
          </div>
          <div v-if="entry.result" class="px-3 py-2 border-t border-border">
            <div class="text-2xs uppercase tracking-wider text-foreground/40 mb-1">
              {{ entry.result.is_error ? 'error' : 'output' }}
            </div>
            <pre
              class="text-2xs font-mono whitespace-pre-wrap break-words max-h-96 overflow-auto"
              :class="entry.result.is_error ? 'text-destructive' : 'text-foreground/70'"
            >{{ entry.result.content }}</pre>
          </div>
        </div>
      </div>

      <!-- Result summary — only shown on failure (or when it carries its own
           text). A successful turn's final message is already rendered as its
           own entry, so the "Completed" box would just be empty noise. -->
      <div
        v-else-if="entry.kind === 'result' && (entry.is_error || entry.text)"
        class="mt-2 rounded-md border p-3"
        :class="entry.is_error
          ? 'border-destructive/30 bg-destructive/5'
          : 'border-success/20 bg-success/5'"
      >
        <div v-if="entry.is_error" class="flex items-center gap-2 mb-2">
          <component :is="XCircle" class="h-4 w-4 text-destructive" :stroke-width="2" />
          <span class="text-xs font-medium text-destructive">Failed</span>
          <div class="ml-auto flex items-center gap-3 text-2xs font-mono text-foreground/40">
            <span v-if="entry.duration_ms">{{ formatDuration(entry.duration_ms) }}</span>
            <span v-if="entry.num_turns">{{ entry.num_turns }} turns</span>
            <span v-if="entry.total_tokens">{{ formatTokens(entry.total_tokens) }} tok</span>
            <span v-if="entry.cost_usd" :title="entry.cost_estimated ? 'Estimated (subscription run)' : undefined">{{ entry.cost_estimated ? '~' : '' }}${{ entry.cost_usd.toFixed(4) }}</span>
          </div>
        </div>
        <div
          v-if="entry.text"
          v-file-links
          v-mermaid
          v-copy-code
          v-html="renderText(entry.text)"
          class="markdown-output text-sm leading-relaxed text-foreground"
          @click="onProseClick"
        />
      </div>

      <!-- diagnostic log line (codex tracing / sidecar notes) -->
      <div
        v-else-if="entry.kind === 'log'"
        class="flex items-start gap-2 text-2xs font-mono text-foreground/45 leading-relaxed"
      >
        <Info class="h-3 w-3 mt-0.75 shrink-0 opacity-50" :stroke-width="1.75" />
        <span
          class="shrink-0 uppercase text-2xs font-semibold tracking-wide px-1 py-px rounded mt-px"
          :class="logBadgeClass(entry.level)"
        >{{ entry.level }}</span>
        <span class="whitespace-pre-wrap break-all">{{ entry.text }}</span>
      </div>

      <!-- stderr line -->
      <div
        v-else-if="entry.kind === 'error'"
        class="flex items-start gap-2 text-xs font-mono text-warning"
      >
        <AlertTriangle class="h-3 w-3 mt-0.5 shrink-0" :stroke-width="1.75" />
        <span class="whitespace-pre-wrap">{{ entry.text }}</span>
      </div>

      <!-- the turn went on on the fallback model (AC-17) -->
      <div
        v-else-if="entry.kind === 'model-fallback'"
        class="my-1 flex items-start gap-2 text-xs text-warning"
      >
        <AlertTriangle class="h-3 w-3 mt-0.5 shrink-0" :stroke-width="1.75" />
        <span>{{ t('streamLog.modelFallback', { fallback: entry.fallback, model: entry.model }) }}</span>
      </div>
      </div><!-- /block rail -->

      <!-- Row of buttons ending a turn: under the question bubble (aligned with
           it, past the avatar), after the answer's last entry. The row reads its
           live state itself, so the memo only tracks where rows are. -->
      <div
        v-if="footers.has(i)"
        :class="footers.get(i)!.side === 'question' ? 'flex justify-end pr-9 -mt-1' : 'mt-1'"
      >
        <MsgActions
          :side="footers.get(i)!.side"
          :has-text="footers.get(i)!.hasText"
          :rewind-turn="footers.get(i)!.rewindTurn"
          :prompt-at="turnPromptAt(footers.get(i)!)"
          :state="msgActionsState"
          :run="(action, from) => runTurnAction(i, action, from)"
        />
      </div>
      </div><!-- /v-memo per-entry wrapper -->
      </template>
      <!-- A turn ending on a tool call ends inside its block: keep its row of
           buttons in view while the block is folded. -->
      <div v-else-if="footers.has(i)" class="mt-1">
        <MsgActions
          :side="footers.get(i)!.side"
          :has-text="footers.get(i)!.hasText"
          :rewind-turn="footers.get(i)!.rewindTurn"
          :prompt-at="turnPromptAt(footers.get(i)!)"
          :state="msgActionsState"
          :run="(action, from) => runTurnAction(i, action, from)"
        />
      </div>
    </template>

    <!-- Running indicator -->
    <div
      v-if="running && !lastEntryIsResult"
      class="thinking-indicator flex items-center gap-2 text-xs font-mono"
    >
      <span class="thinking-icon inline-flex">
        <Sparkles class="h-3 w-3 text-emerald-400" :stroke-width="1.75" />
      </span>
      <span v-if="liveStatusText" :class="liveStatus?.kind === 'retry' ? 'text-warning' : 'text-foreground/60'">{{ liveStatusText }}</span>
      <span v-else class="thinking-text">thinking</span>
      <span class="thinking-dots" aria-hidden="true">
        <span class="thinking-dot"></span>
        <span class="thinking-dot"></span>
        <span class="thinking-dot"></span>
      </span>
    </div>

    <MessageFullscreen
      v-if="turnActionsOn"
      :open="!!fullscreen"
      :title="fullscreen?.title ?? ''"
      :sources="fullscreen ? [fullscreen.text] : []"
      :owner="owner"
      @close="fullscreen = null"
    >
      <template v-if="fullscreen">
        <StreamLog
          v-if="fullscreen.entries"
          no-turn-actions
          :entries="fullscreen.entries"
          :render-text="renderText"
          :file-matcher="fileMatcher"
          :file-exists="fileExists"
          :file-list-version="fileListVersion"
          @open-file="(path, line) => emit('open-file', path, line)"
          @open-url="(url) => emit('open-url', url)"
        />
        <div v-else-if="fullscreen.plain" class="whitespace-pre-wrap text-sm leading-relaxed text-foreground">{{ fullscreen.text }}</div>
        <div
          v-else
          v-file-links
          v-mermaid
          v-copy-code
          v-html="renderText(fullscreen.text)"
          class="markdown-output text-sm leading-relaxed text-foreground"
          @click="onProseClick"
        />
      </template>
    </MessageFullscreen>
  </div>
</template>

<style scoped>
/* A turn's folded work hangs from a rail under its header. The log spaces its
   entries with `space-y-3` (a bottom margin); on the rail that gap becomes
   padding instead, so the line runs unbroken from one step to the next. */
.turn-activity-open {
  margin-block-end: 0.125rem;
}
.turn-step-wrap {
  margin-block-end: 0;
}
.turn-step {
  margin-left: 0.6rem;
  padding-left: 0.85rem;
  border-left: 1px solid hsl(var(--border));
}
.turn-step:not(.turn-step-last) {
  padding-block-end: 0.75rem;
}

/* Icon: gentle breathing glow + subtle rotation to feel "alive". */
.thinking-icon {
  animation: thinking-icon-pulse 1.8s ease-in-out infinite;
  transform-origin: center;
}

@keyframes thinking-icon-pulse {
  0%, 100% {
    opacity: 0.55;
    transform: scale(0.92) rotate(-6deg);
    filter: drop-shadow(0 0 0 rgba(52, 211, 153, 0));
  }
  50% {
    opacity: 1;
    transform: scale(1.08) rotate(6deg);
    filter: drop-shadow(0 0 4px rgba(52, 211, 153, 0.5));
  }
}

/* Text: animated shimmer sweeping across the word. */
.thinking-text {
  background: linear-gradient(
    100deg,
    rgba(148, 163, 184, 0.45) 30%,
    rgba(52, 211, 153, 0.95) 50%,
    rgba(148, 163, 184, 0.45) 70%
  );
  background-size: 220% 100%;
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
  animation: thinking-shimmer 2s linear infinite;
}

@keyframes thinking-shimmer {
  0% { background-position: 180% 0; }
  100% { background-position: -80% 0; }
}

/* Trailing dots that bounce in sequence. */
.thinking-dots {
  display: inline-flex;
  align-items: flex-end;
  gap: 3px;
  margin-left: -2px;
  padding-bottom: 1px;
}

.thinking-dot {
  width: 3px;
  height: 3px;
  border-radius: 9999px;
  background-color: rgb(52, 211, 153);
  animation: thinking-bounce 1.4s ease-in-out infinite;
}

.thinking-dot:nth-child(2) {
  animation-delay: 0.2s;
}

.thinking-dot:nth-child(3) {
  animation-delay: 0.4s;
}

@keyframes thinking-bounce {
  0%, 80%, 100% {
    transform: translateY(0);
    opacity: 0.4;
  }
  40% {
    transform: translateY(-4px);
    opacity: 1;
  }
}

/* Respect reduced-motion preferences: keep it readable, drop the motion. */
@media (prefers-reduced-motion: reduce) {
  .thinking-icon,
  .thinking-text,
  .thinking-dot {
    animation: none;
  }
  .thinking-text {
    color: rgba(148, 163, 184, 0.7);
  }
}
</style>
