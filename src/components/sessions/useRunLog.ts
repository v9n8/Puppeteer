// The AI result pane: the persisted-log window (paging, jumps, reload on focus),
// the live output's bottom pin and scroll anchoring, the prompt navigator and
// scroll memory, bookmarks, rewind, fork and ⌘F over the whole log. Part of the
// RunView split (see useRunSession.ts).
import { ref, shallowRef, computed, onUnmounted, watch, nextTick } from 'vue'
import {
  sameRunLogRevision, type RunLogRevision, type RunLogHit, type RewindOutcome, type RunRecord,
} from '@/stores/runs'
import { forkErrorKey } from '@/lib/runFork'
import { usePageFind } from '@/composables/usePageFind'
import type { RouteHit } from '@/lib/sessionSearch'
import { useTurnNavigator, turnLabel } from '@/composables/useTurnNavigator'
import { isAwayFromTop, promptsBefore } from '@/lib/turnPosition'
import { useScrollMemory } from '@/composables/useScrollMemory'
import { bookmarkKey, placeBookmarks, type RunBookmark } from '@/lib/bookmarks'
import { pendingRunQuotes, quoteSetViews, type DraftQuote, type RunQuote } from '@/lib/quote'
import { flashQuoteSource, quoteHosts, quoteRangeIn } from '@/lib/quoteAnchor'
import type { TurnSide } from '@/lib/turnActions'
import { openSessionWindow } from '@/lib/sessionWindow'
import { useMarkdown } from '@/lib/markdown'
import {
  entriesToPlainText, parseStreamLogWindow, modelFromPreamble, createStreamState, applyStreamEvent,
  type StreamEntry,
} from '@/lib/streamEvents'
import type { RunSessionCore } from './useRunSessionCore'

/** Composer members the log reads before the composer exists. createRunSession
 * fills them in right after; they are only called once setup has finished. */
export interface RunLogLate {
  closeRunSettingsMenu: () => void
  dismissSelection: () => void
  /** A launch on another run takes this run's effort / style picks along. */
  carryPicks: (fromRunId: string, toRunId: string) => void
}

/** Where the Rewind dialog goes back to, and for whom. */
export interface RewindTarget {
  runId: string
  turn: number
  /** The rewind of an edited message's Send (UX-30): the dialog says it sends next. */
  editResend?: boolean
  /** Told how it ended: true = the conversation went back and the log reloaded; false = closed, or files only. */
  settle?: (rewound: boolean) => void
}

type ScrollAnchor = { bottom: true } | { el: Element; offset: number } | null
type SessionTurn = { label: string; offset: number }

export function useRunLog(core: RunSessionCore, late: RunLogLate) {
  const {
    t, route, router, runsStore, live, features, uiLayout, toast, confirm, projectId, activeRunId,
    syncLoadedRunEngine, currentRunId, currentIsSession, liveEntries, liveOutputLines, liveHasStream,
    currentStatus, isSessionWindow,
  } = core

  // Open a worker session (from the conductor sidebar) IN-PLACE, replacing the main
  // view in the current window — the worker panel stays visible (anchored to the
  // conductor, see conductorAnchorId) so the user can hop between workers or back to
  // the conductor without a forest of OS windows.
  async function openSession(runId: string) {
    if (!runId) return
    await loadRunLog(runId)
  }
  // Secondary action: pop a worker out into its OWN OS window (one window per run),
  // for watching several workers side by side. Clicking the same worker again just
  // focuses the window already showing it.
  async function popoutSession(runId: string) {
    if (!runId) return
    await openSessionWindow(projectId.value, runId)
  }

  const outputEl = ref<HTMLDivElement | null>(null)
  const historyEl = ref<HTMLDivElement | null>(null)
  const sessionListEl = ref<HTMLDivElement | null>(null)

  // Scroll the session-history list so the currently open session is visible.
  // Called whenever the active run changes (click, deep link, back/forward).
  function scrollActiveSessionIntoView() {
    const id = currentRunId.value
    if (!id) return
    nextTick(() => {
      const container = sessionListEl.value
      if (!container) return
      const el = container.querySelector<HTMLElement>(`[data-run-id="${CSS.escape(id)}"]`)
      if (el) el.scrollIntoView({ block: 'nearest' })
    })
  }

  const viewingLogRunId = ref<string | null>(null)
  const viewingLog = ref<string>('')

  // History (on-disk log) view state — only used when viewing a finished run that
  // has no in-memory live session. Kept separate from the live session so a
  // background run can keep streaming into its own buffer.
  // shallowRef: a persisted log is immutable once parsed, so we never mutate an
  // entry in place. Skipping Vue's deep-reactive proxy over the whole (possibly
  // large) array makes parsing/assigning + the first render markedly cheaper.
  const historyEntries = shallowRef<StreamEntry[]>([])
  const historyHasStream = ref(false)
  // True while reading + parsing a persisted log from disk, so the viewer can show
  // a loading animation instead of a blank/"Loading…" flash. Skipped on a focus
  // refresh (we keep the current content on screen and swap it in silently).
  const historyLoading = ref(false)
  const historyToolIndex = new Map<string, number>()
  // Context-window state reconstructed from a persisted log (history view).
  const historyContextTokens = ref(0)
  const historyModel = ref<string | null>(null)

  // ── History paging ──────────────────────────────────────────────────────────
  // Opening a run used to read the WHOLE log (up to ~29MB) over IPC and parse all
  // of it, only to render the last 80 entries. Now the backend returns one window
  // of records at a time and `historyEntries` holds exactly what has been loaded —
  // it is no longer a full-log array with a slice on top.
  //
  // `historyCursor` is the byte offset the oldest loaded record starts at; feeding
  // it back as `before` fetches the window just older than it.
  const HISTORY_PAGE_INITIAL = 80
  const HISTORY_PAGE_STEP = 120
  const historyCursor = ref<number | null>(null)
  const historyHasMore = ref(false)
  const historyLoadingMore = ref(false)
  // The window can sit mid-file after a jump (not just the tail), so we also track
  // the BOTTOM edge: `historyBottomCursor` is the byte offset just past the last
  // loaded record, `historyHasMoreBelow` is true when newer content remains on
  // disk below it, and `historyFileSize` is the log's size (to compute the above).
  const historyBottomCursor = ref<number | null>(null)
  const historyHasMoreBelow = ref(false)
  const historyFileSize = ref(0)
  // Session-prompt index of the FIRST loaded turn, tracked only while the window is
  // mid-file (after a jump). At the tail it is derived as a suffix instead — see
  // `loadedTurnOffset`. Lets the navigator map loaded turns back to the full list
  // even when the window isn't anchored to the end.
  const firstLoadedSeq = ref(0)
  // Briefly after a jump, ignore scroll-driven auto-loading: the programmatic
  // scroll-to-top would otherwise instantly trigger a prepend and shove the landing
  // out of view.
  let historyAutoLoadBlockedUntil = 0
  // Model id recovered from the page preamble (the `system.init` record at the top
  // of the log). A tail window cannot contain it, and without it the context-window
  // limit is unknown.
  const historyPreambleModel = ref<string | null>(null)
  // Fingerprint of the log currently on screen, so a refocus can skip the read
  // entirely when nothing changed.
  const historyRevision = ref<RunLogRevision | null>(null)
  // Tool entries covering the WHOLE persisted run (not just the loaded window),
  // used only to build the "files changed" list. shallowRef + a pruned payload
  // keeps this small: the backend strips file bodies and tool output before
  // sending, so this is orders of magnitude smaller than the log it came from.
  const historyToolEntries = shallowRef<StreamEntry[]>([])

  /**
   * Populate `historyToolEntries` for a persisted run.
   *
   * Fire-and-forget on purpose: the run view paints from its own page load, and
   * the "files changed" button simply appears once this resolves. Failing is
   * non-fatal — the button stays hidden rather than blocking the view.
   */
  async function loadHistoryToolEntries(runId: string) {
    try {
      const records = await runsStore.getRunToolRecords(runId)
      if (viewingLogRunId.value !== runId) return
      historyToolEntries.value = parseStreamLogWindow(records).entries
    } catch {
      historyToolEntries.value = []
    }
  }

  // Load the window just older than what's on screen and prepend it, preserving
  // the viewport: older entries push everything down, so bump scrollTop by exactly
  // the height they added (measured across the next frame) and the content the
  // user was reading stays put.
  async function showEarlierHistory() {
    const runId = viewingLogRunId.value
    if (!runId || !historyHasMore.value || historyLoadingMore.value) return
    if (historyCursor.value === null) return
    historyLoadingMore.value = true
    const el = historyEl.value
    const beforeHeight = el?.scrollHeight ?? 0
    const beforeTop = el?.scrollTop ?? 0
    try {
      const page = await runsStore.getRunLogPage(runId, historyCursor.value, HISTORY_PAGE_STEP)
      // The run may have been switched while this was in flight.
      if (viewingLogRunId.value !== runId) return
      const parsed = parseStreamLogWindow(page.records, { seedModel: historyPreambleModel.value })
      historyCursor.value = page.cursor
      historyHasMore.value = page.has_more
      if (parsed.entries.length) {
        historyEntries.value = [...parsed.entries, ...historyEntries.value]
        // Older turns now sit before the window's first, so the first-loaded index
        // moves back by however many prompts we just prepended.
        const added = parsed.entries.reduce((n, e) => n + (e.kind === 'user' ? 1 : 0), 0)
        firstLoadedSeq.value = Math.max(0, firstLoadedSeq.value - added)
      }
      await nextTick()
      requestAnimationFrame(() => {
        const e = historyEl.value
        if (!e) return
        e.scrollTop = beforeTop + (e.scrollHeight - beforeHeight)
      })
    } catch {
      /* leave what's already loaded on screen */
    } finally {
      historyLoadingMore.value = false
    }
  }

  // Append the next window of newer records below the loaded ones — the mirror of
  // `showEarlierHistory`. Needed once the window can sit mid-file (after a jump),
  // so scrolling down past it keeps reading toward the end. New content lands below
  // the viewport, so no scroll compensation is required.
  async function showLaterHistory() {
    const runId = viewingLogRunId.value
    if (!runId || !historyHasMoreBelow.value || historyLoadingMore.value) return
    if (historyBottomCursor.value === null) return
    historyLoadingMore.value = true
    try {
      const page = await runsStore.getRunLogForward(runId, historyBottomCursor.value, HISTORY_PAGE_STEP)
      if (viewingLogRunId.value !== runId) return
      const parsed = parseStreamLogWindow(page.records, { seedModel: historyPreambleModel.value })
      historyBottomCursor.value = page.cursor
      historyHasMoreBelow.value = page.has_more
      if (parsed.entries.length) {
        historyEntries.value = [...historyEntries.value, ...parsed.entries]
      }
    } catch {
      /* leave what's already loaded on screen */
    } finally {
      historyLoadingMore.value = false
    }
  }

  // Replace the loaded window with a fresh one starting at a byte offset (the start
  // of a prompt record, from the whole-session scan). One forward read instead of
  // paging back from the tail — the heavy part of jumping to an old prompt.
  async function jumpToHistoryOffset(offset: number) {
    const runId = viewingLogRunId.value
    if (!runId) return
    historyLoadingMore.value = true
    try {
      const page = await runsStore.getRunLogForward(runId, offset, HISTORY_PAGE_STEP)
      if (viewingLogRunId.value !== runId) return
      const parsed = parseStreamLogWindow(page.records, { seedModel: historyPreambleModel.value })
      historyEntries.value = parsed.entries
      historyCursor.value = offset
      historyHasMore.value = offset > 0
      historyBottomCursor.value = page.cursor
      historyHasMoreBelow.value = page.has_more
      historyFileSize.value = page.revision.size
      // The programmatic scroll that follows sits at the very top → would trip the
      // scroll-up auto-loader; hold it off briefly.
      historyAutoLoadBlockedUntil = Date.now() + 500
    } catch {
      /* leave what's already loaded on screen */
    } finally {
      historyLoadingMore.value = false
    }
  }

  const inputContent = ref<string>('')
  const inputContentRunId = ref<string | null>(null)
  const inputContentError = ref<string | null>(null)
  const inputContentLoading = ref(false)

  const { renderText, loadMarkdown } = useMarkdown()

  // `viewingLogRunId` is set only while showing an on-disk log (it's cleared the
  // moment a run goes live), so its presence alone means we're in history view.
  // This also surfaces a partial log recovered from disk for a run left as
  // 'running' by a previous app session that died mid-run.
  const isViewingHistory = computed(() => !!viewingLogRunId.value)
  // Entries currently shown in the AI Result column — drives the "files changed"
  // quick-access list so it matches whatever the user is looking at.
  // Entries the "files changed" list is derived from.
  //
  // It must reflect the WHOLE run, not the window currently on screen — otherwise
  // a long conversation silently under-reports what it changed. For a live session
  // the in-memory entries already are the whole run. For a persisted one they are
  // only a window, so the backend scans the log and returns the (pruned) tool
  // records instead; see `loadHistoryToolEntries`.
  const mentionedFileEntries = computed(() =>
    isViewingHistory.value ? historyToolEntries.value : liveEntries.value,
  )
  const hasLiveOutput = computed(
    () => liveOutputLines.value.length > 0 || liveEntries.value.length > 0 || currentStatus.value === 'running'
  )
  // Split-pane state: percentage of the left (Content) column.
  const splitContainerEl = ref<HTMLElement | null>(null)
  const leftWidthPct = ref(50)
  const isResizing = ref(false)

  // Container for the AI Result column (left: AI result, right: optional content).
  const resultSplitEl = ref<HTMLElement | null>(null)

  // Panel visibility: the AI Result column is always on — issue/PR runs can pull
  // the fetched Content in as a left split column via the header toggle. Sessions
  // have no issue/PR content, so Content is never available for them.
  const showContent = ref(false)
  const canToggleContent = computed(() => !uiLayout.focusMode && !currentIsSession.value)
  const contentVisible = computed(() => canToggleContent.value && showContent.value)
  const showResizeHandle = computed(() => contentVisible.value)
  // No run is selected AND the project has none — hide the Content / AI Result
  // split entirely and show a single centered "No sessions yet" empty state.
  const noSession = computed(() => !currentRunId.value && runsStore.runs.length === 0)
  const contentWidth = computed(() => (showResizeHandle.value ? leftWidthPct.value + '%' : '100%'))
  const resultWidth = computed(() => (showResizeHandle.value ? 100 - leftWidthPct.value + '%' : '100%'))

  function startResize(e: MouseEvent) {
    e.preventDefault()
    isResizing.value = true
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'
    window.addEventListener('mousemove', onResize)
    window.addEventListener('mouseup', stopResize)
  }

  function onResize(e: MouseEvent) {
    if (!splitContainerEl.value) return
    const rect = splitContainerEl.value.getBoundingClientRect()
    const pct = ((e.clientX - rect.left) / rect.width) * 100
    leftWidthPct.value = Math.max(20, Math.min(80, pct))
  }

  function stopResize() {
    isResizing.value = false
    document.body.style.cursor = ''
    document.body.style.userSelect = ''
    window.removeEventListener('mousemove', onResize)
    window.removeEventListener('mouseup', stopResize)
  }

  const sourceText = computed(() => {
    if (isViewingHistory.value) {
      if (historyHasStream.value) return entriesToPlainText(historyEntries.value)
      return viewingLog.value
    }
    if (liveHasStream.value) return entriesToPlainText(liveEntries.value)
    return liveOutputLines.value.map(l => l.text).join('\n')
  })

  // Legacy (non-stream) markdown for each view column.
  const liveLegacyHtml = computed(() => liveHasStream.value ? '' : renderText(
    liveOutputLines.value.map(l => l.text).join('\n')
  ))
  const historyLegacyHtml = computed(() => historyHasStream.value ? '' : renderText(viewingLog.value))

  // Whether the live output is "pinned" to the bottom. We only auto-scroll when
  // the user is already near the bottom; if they scroll up to read earlier
  // output, we leave their position alone until they come back down.
  const stickToBottom = ref(true)
  const SCROLL_PIN_THRESHOLD = 80 // px from the bottom counted as "at bottom"

  function isNearBottom(el: HTMLElement) {
    return el.scrollHeight - el.scrollTop - el.clientHeight <= SCROLL_PIN_THRESHOLD
  }

  function scrollOutputToBottom() {
    nextTick(() => {
      if (outputEl.value) outputEl.value.scrollTop = outputEl.value.scrollHeight
    })
  }

  // Re-evaluate the pin whenever the user scrolls the live output.
  function onOutputScroll() {
    scheduleScrollSettle()
    scrollMemory.schedule()
    scheduleTurnPositionCheck()
  }

  // --- Scroll anchoring across layout resizes ---------------------------------
  // When the question panel opens/closes (or the split is dragged), the output
  // column changes width, text re-wraps and the total scrollHeight changes. A
  // pixel-based scrollTop then points at different content, so the reader jumps.
  // We fix this by anchoring on an *element* near the top of the viewport and
  // restoring scrollTop after the reflow so that element stays put.
  let scrollAnchor: ScrollAnchor = null

  // The scrollable output may wrap its content in a single container (StreamLog's
  // `.space-y-3` root, or the legacy markdown div). Descend through single-child
  // wrappers to reach the actual list of block entries we can anchor on.
  function outputBlocks(root: HTMLElement): Element[] {
    let node: Element = root
    while (node.children.length === 1) node = node.children[0]
    return Array.from(node.children)
  }

  function captureScrollAnchor() {
    const el = outputEl.value
    if (!el) return
    // If pinned to the bottom, keep it pinned — no element anchor needed.
    if (stickToBottom.value) { scrollAnchor = { bottom: true }; return }
    const top = el.getBoundingClientRect().top
    for (const node of outputBlocks(el)) {
      const r = node.getBoundingClientRect()
      if (r.height > 0 && r.bottom >= top) {
        scrollAnchor = { el: node, offset: r.top - top }
        return
      }
    }
    scrollAnchor = null
  }

  // Everything a scroll has to recompute reads layout: `scrollHeight` for the
  // bottom pin, entry rects for the anchor. A trackpad emits several scroll events
  // per frame, and each of those reads forces a synchronous layout mid-scroll —
  // that is the stutter. Coalesce into one read per frame; both values only have
  // to be current by the time the frame is painted.
  let scrollSettleFrame = 0
  function scheduleScrollSettle() {
    if (scrollSettleFrame) return
    scrollSettleFrame = requestAnimationFrame(() => {
      scrollSettleFrame = 0
      // Pointer interaction pauses auto-follow. Do not silently re-enable it while
      // the pointer is still down (for example while drag-selecting text).
      if (outputEl.value && !pointerDownInOutput.value) {
        stickToBottom.value = isNearBottom(outputEl.value)
      }
      captureScrollAnchor()
    })
  }

  function restoreScrollAnchor() {
    const el = outputEl.value
    const a = scrollAnchor
    if (!el || !a) return
    if ('bottom' in a) { el.scrollTop = el.scrollHeight; return }
    const top = el.getBoundingClientRect().top
    const delta = (a.el.getBoundingClientRect().top - top) - a.offset
    if (delta) el.scrollTop += delta
  }

  // Attach a ResizeObserver to the output element whenever it mounts, so any
  // width/height change restores the reading position. The output appears and
  // disappears via v-if, so we track the ref rather than observing once at mount.
  let outputRO: ResizeObserver | null = null
  // Streaming text grows the last entry in place (and StreamLog reveals it over
  // several frames), so the entry count the watcher below keys on stays flat.
  // Follow the content's actual height instead; the child list is re-observed
  // when the output switches between StreamLog and the legacy markdown view.
  let contentRO: ResizeObserver | null = null
  let contentMO: MutationObserver | null = null
  watch(outputEl, (el) => {
    outputRO?.disconnect()
    outputRO = null
    contentRO?.disconnect()
    contentRO = null
    contentMO?.disconnect()
    contentMO = null
    if (el) {
      outputRO = new ResizeObserver(() => restoreScrollAnchor())
      outputRO.observe(el)
      const ro = new ResizeObserver(() => keepPinnedToBottom())
      const observeChildren = () => {
        ro.disconnect()
        for (const child of Array.from(el.children)) ro.observe(child)
      }
      observeChildren()
      contentRO = ro
      contentMO = new MutationObserver(observeChildren)
      contentMO.observe(el, { childList: true })
    }
  })

  // ── Jump between user prompts ───────────────────────────────────────────────
  // In a long run, scrolling back to "where did I ask for this?" is a long drag
  // past tool calls and assistant output. The user's own messages are the
  // landmarks worth landing on, so they get their own navigation. It drives
  // whichever pane is showing — the live output or a history log.
  const turnScrollEl = computed(() => (isViewingHistory.value ? historyEl.value : outputEl.value))
  const turnEntries = computed(() => (isViewingHistory.value ? historyEntries.value : liveEntries.value))
  const {
    turns: promptTurns,
    scrollToTurn,
    activeEntryIndex,
    firstTurnTop,
  } = useTurnNavigator(turnScrollEl, turnEntries)

  // The navigator must list EVERY prompt in the session, but the viewer only
  // holds a window (the tail of a history log), so `promptTurns` — derived from
  // what's on screen — under-reports. `sessionTurns` is the whole-session list,
  // scanned from the log in Rust and parsed with the SAME parser (see
  // `loadSessionTurns`); empty until that resolves, when we fall back to the
  // loaded turns.
  const sessionTurns = shallowRef<SessionTurn[]>([])

  async function loadSessionTurns(runId: string) {
    try {
      const records = await runsStore.getRunUserRecords(runId)
      if (currentRunId.value !== runId) return
      // Feed the pruned user records through the SAME parser the main view uses,
      // one at a time, so compaction summaries / slash-command messages are
      // filtered identically — and tag each resulting prompt with the byte offset
      // of the record that produced it (for jump-to-offset). A real user prompt is
      // always the last entry pushed by its record, so identity-compare the tail.
      const state = createStreamState()
      const turns: SessionTurn[] = []
      let lastUser: StreamEntry | null = null
      for (const r of records) {
        try {
          applyStreamEvent(state, JSON.parse(r.json))
        } catch {
          continue
        }
        const last = state.entries[state.entries.length - 1]
        if (last && last.kind === 'user' && last !== lastUser) {
          lastUser = last
          turns.push({ label: turnLabel(last.text), offset: r.offset })
        }
      }
      sessionTurns.value = turns
    } catch {
      if (currentRunId.value === runId) sessionTurns.value = []
    }
  }

  // Rescan whenever the focused run changes. The scan reads the persisted log that
  // both the live and history panes share, so it covers either mode.
  watch(
    currentRunId,
    (id) => {
      sessionTurns.value = []
      if (id) void loadSessionTurns(id)
    },
    { immediate: true },
  )

  // The list that actually drives the navigator: the whole-session scan when it's
  // at least as complete as what's loaded, otherwise the loaded turns (covers a
  // live run that just produced a prompt the scan predates, and the pre-scan gap).
  const navTurns = computed<{ label: string; offset?: number }[]>(() =>
    sessionTurns.value.length >= promptTurns.value.length && sessionTurns.value.length > 0
      ? sessionTurns.value
      : promptTurns.value.map((t) => ({ label: t.label })),
  )

  // Session-prompt index of the loaded window's first turn. While more content
  // remains BELOW (a mid-file window, i.e. after a jump), that index is tracked
  // explicitly in `firstLoadedSeq`. Otherwise the window reaches the end, so the
  // loaded turns are the tail — a plain suffix of the full list — which also stays
  // correct for the live pane.
  const loadedTurnOffset = computed(() =>
    historyHasMoreBelow.value
      ? firstLoadedSeq.value
      : Math.max(0, navTurns.value.length - promptTurns.value.length),
  )

  // Remember where the reader was in each session (anchored on these same prompt
  // landmarks) so reopening it — from its project tab, the History list or after
  // a reload — lands back on that spot instead of the end of the log.
  const scrollMemory = useScrollMemory({
    runId: currentRunId,
    projectId,
    scrollEl: turnScrollEl,
    turns: promptTurns,
    loadedTurnOffset,
    navTurns,
    activeEntryIndex,
    isNearBottom,
    isReady: () =>
      isViewingHistory.value
        ? !historyLoading.value && (historyEntries.value.length > 0 || !!viewingLog.value)
        : liveEntries.value.length > 0 || liveOutputLines.value.length > 0,
    jumpTo: async (seq, offset) => {
      if (!isViewingHistory.value || !historyHasStream.value) return
      firstLoadedSeq.value = seq
      await jumpToHistoryOffset(offset)
    },
  })

  // One landmark is nothing to navigate between, and the legacy (non-stream)
  // renderer has no turn markers to walk.
  const showTurnNav = computed(
    () =>
      navTurns.value.length > 1 &&
      !!turnScrollEl.value &&
      (isViewingHistory.value ? historyHasStream.value : liveHasStream.value),
  )

  // Which prompt (as an index into the full `navTurns`) the reader is parked on,
  // marked in the jump list. Resolved when the menu opens, so the highlight is
  // exact even if the last per-frame counter check (below) hasn't run yet.
  const activeTurnSeq = ref(-1)
  function loadedTurnIndexOf(entryIndex: number): number {
    return promptTurns.value.findIndex((t) => t.entryIndex === entryIndex)
  }
  /** 1-based place in the session of the prompt at loaded entry `entryIndex`; null when unknown. */
  function promptNumber(entryIndex: number): number | null {
    const li = loadedTurnIndexOf(entryIndex)
    return li < 0 ? null : loadedTurnOffset.value + li + 1
  }
  // Full-list index of the turn currently at the top, or -1 when above the first
  // loaded one.
  function activeSessionSeq(): number {
    const li = loadedTurnIndexOf(activeEntryIndex())
    return li < 0 ? -1 : loadedTurnOffset.value + li
  }
  function onTurnMenuToggle(open: boolean) {
    if (open) activeTurnSeq.value = activeSessionSeq()
  }

  // The `n/N` counter and the "Top" button follow the scroll, so unlike the jump
  // list they are re-measured as the reader moves — at most once per frame, like
  // `scheduleHistoryBottomCheck`. `activeEntryIndex` is a binary search, so a
  // frame reads a few rects, not every turn's.
  const turnPositionTracked = computed(
    () =>
      !!turnScrollEl.value &&
      navTurns.value.length > 0 &&
      (isViewingHistory.value ? historyHasStream.value && !historyLoading.value : liveHasStream.value),
  )
  const currentTurnSeq = ref(-1)
  const turnAwayFromTop = ref(false)
  let turnPositionFrame = 0
  function scheduleTurnPositionCheck() {
    if (turnPositionFrame) return
    turnPositionFrame = requestAnimationFrame(() => {
      turnPositionFrame = 0
      if (!turnPositionTracked.value) {
        currentTurnSeq.value = -1
        turnAwayFromTop.value = false
        return
      }
      currentTurnSeq.value = activeSessionSeq()
      // Only the history pane can hold a window that starts mid-session; the live
      // pane's top is simply the top of what it shows.
      turnAwayFromTop.value = isAwayFromTop(
        isViewingHistory.value && loadedTurnOffset.value > 0,
        firstTurnTop(),
      )
    })
  }
  // Scrolling covers most moves; this covers the ones that shift the turns
  // without a scroll event (a window loaded, a pane switched, a first prompt).
  watch(
    [turnPositionTracked, turnScrollEl, () => promptTurns.value.length, loadedTurnOffset],
    scheduleTurnPositionCheck,
    { flush: 'post' },
  )

  // "Top": back to the session's first prompt. The history pane may hold only a
  // window of the log, so it jumps by offset (one window read) rather than
  // paging the whole log into the DOM; the live pane has no window to load, so
  // its top is just the top of what it shows.
  function scrollToFirstTurn() {
    if (isViewingHistory.value) {
      void jumpToSessionTurn(0)
      return
    }
    const el = outputEl.value
    if (!el) return
    el.scrollTop = 0
    afterTurnJump()
  }

  // Jumping away from the bottom has to release the auto-follow pin, or the next
  // streamed chunk yanks the reader straight back down.
  function afterTurnJump() {
    const el = outputEl.value
    if (el && !isViewingHistory.value) stickToBottom.value = isNearBottom(el)
  }

  function goToTurn(entryIndex: number, smooth = false) {
    scrollToTurn(entryIndex, smooth)
    afterTurnJump()
  }

  // Jump to a prompt by its position in the WHOLE session. `smooth` glides to the
  // landing — used for an explicit pick from the list; prev/next step instantly so
  // rapid presses stay snappy.
  async function jumpToSessionTurn(seq: number, smooth = false) {
    if (seq < 0 || seq >= navTurns.value.length) return

    // Already in the loaded window → just scroll, no reload.
    const localIfLoaded = seq - loadedTurnOffset.value
    if (localIfLoaded >= 0 && localIfLoaded < promptTurns.value.length) {
      goToTurn(promptTurns.value[localIfLoaded].entryIndex, smooth)
      return
    }

    // History + we know the prompt's byte offset → load a fresh window right there
    // in one read, then land on it (now the window's first turn).
    const offset = navTurns.value[seq]?.offset
    if (isViewingHistory.value && typeof offset === 'number') {
      firstLoadedSeq.value = seq
      await jumpToHistoryOffset(offset)
      await nextTick()
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
      const turn = promptTurns.value[seq - loadedTurnOffset.value]
      if (turn) goToTurn(turn.entryIndex, smooth)
      return
    }

    // Fallback (live pane, or offset not scanned yet): page backwards until the
    // target is in, then scroll.
    if (isViewingHistory.value) {
      let guard = 0
      while (seq < loadedTurnOffset.value && historyHasMore.value && !historyLoadingMore.value && guard < 500) {
        await showEarlierHistory()
        guard++
      }
      await nextTick()
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
    }
    const turn = promptTurns.value[seq - loadedTurnOffset.value]
    if (turn) goToTurn(turn.entryIndex, smooth)
  }

  function prevTurn() {
    const cur = activeSessionSeq()
    // -1 means the reader is above the first loaded turn — step to the one just
    // before the loaded window.
    const target = cur < 0 ? loadedTurnOffset.value - 1 : cur - 1
    if (target >= 0) void jumpToSessionTurn(target)
  }

  function nextTurn() {
    const cur = activeSessionSeq()
    const target = cur < 0 ? loadedTurnOffset.value : cur + 1
    if (target < navTurns.value.length) void jumpToSessionTurn(target)
  }

  // ⌘/Ctrl+⌥+↑/↓. The bare arrows — and their plain ⌘ or ⌥ variants — all move the
  // caret inside the composer's textarea, so the shortcut takes the one
  // combination both macOS and the text field leave free.
  function onTurnNavKey(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey) || !e.altKey) return
    if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return
    if (!showTurnNav.value) return
    e.preventDefault()
    if (e.key === 'ArrowUp') prevTurn()
    else nextTurn()
  }

  // While the user is actively interacting with the output — holding the mouse
  // down to drag-select, or with text already selected inside it — we must NOT
  // yank the view to the bottom when new streamed content arrives. Otherwise a
  // click/selection near the bottom gets interrupted the moment a new chunk lands.
  const pointerDownInOutput = ref(false)
  function onOutputPointerDown() {
    pointerDownInOutput.value = true
    // A click means the user is reading/interacting with the current content.
    // Pause auto-follow until they deliberately scroll back to the bottom.
    stickToBottom.value = false
  }
  function onWindowPointerUp() {
    pointerDownInOutput.value = false
  }
  function hasOutputSelection() {
    const sel = window.getSelection()
    if (!sel || sel.isCollapsed || !outputEl.value) return false
    return (
      (sel.anchorNode != null && outputEl.value.contains(sel.anchorNode)) ||
      (sel.focusNode != null && outputEl.value.contains(sel.focusNode))
    )
  }

  // Auto-load the history window as the reader approaches either edge: older
  // records when near the top, newer ones when near the bottom. Held off briefly
  // right after a jump (the programmatic scroll would otherwise instantly prepend).
  const HISTORY_SCROLL_MARGIN = 400
  // Whether the history pane is scrolled up from the end of the log — shows the
  // jump-to-latest button (a reopened session may land mid-way). One layout read
  // per frame, like the live pane's pin.
  const historyAwayFromBottom = ref(false)
  let historyBottomFrame = 0
  function scheduleHistoryBottomCheck() {
    if (historyBottomFrame) return
    historyBottomFrame = requestAnimationFrame(() => {
      historyBottomFrame = 0
      const el = historyEl.value
      historyAwayFromBottom.value = !!el && !isNearBottom(el)
    })
  }

  // Jump-to-latest for the history pane. A window loaded mid-file (after a jump
  // or a restored position) doesn't reach the end, so reload the tail instead.
  function scrollHistoryToLatest() {
    const id = viewingLogRunId.value
    if (!id) return
    scrollMemory.forget(id)
    if (historyHasMoreBelow.value) {
      void loadRunLog(id)
      return
    }
    if (historyEl.value) historyEl.value.scrollTop = historyEl.value.scrollHeight
  }

  function onHistoryScroll() {
    scrollMemory.schedule()
    scheduleHistoryBottomCheck()
    scheduleTurnPositionCheck()
    const el = historyEl.value
    if (!el || historyLoadingMore.value || Date.now() < historyAutoLoadBlockedUntil) return
    if (el.scrollTop < HISTORY_SCROLL_MARGIN && historyHasMore.value) {
      void showEarlierHistory()
    } else if (
      el.scrollHeight - el.scrollTop - el.clientHeight < HISTORY_SCROLL_MARGIN &&
      historyHasMoreBelow.value
    ) {
      void showLaterHistory()
    }
  }

  // ── Bookmarks ────────────────────────────────────────────────────────────────
  // Anchored on the session's prompt list (`sessionTurns`: byte offset + label of
  // each prompt record), never on the DOM, which only holds a window of the log.
  const bookmarks = shallowRef<RunBookmark[]>([])

  async function loadBookmarks(runId: string | null) {
    if (!runId) return
    try {
      const rows = await runsStore.listBookmarks(runId)
      if (currentRunId.value === runId) bookmarks.value = rows
    } catch {
      if (currentRunId.value === runId) bookmarks.value = []
    }
  }

  watch(
    currentRunId,
    (id) => {
      bookmarks.value = []
      void loadBookmarks(id)
    },
    { immediate: true },
  )

  const placedBookmarks = computed(() => placeBookmarks(bookmarks.value, sessionTurns.value))
  const bookmarkByTurn = computed(
    () => new Map(placedBookmarks.value.map((p) => [bookmarkKey(p.seq, p.bookmark.side), p.bookmark])),
  )

  // The session prompt a row's turn starts with: loaded entry `promptAt`, or the
  // prompt just before the loaded window (-1). Null until the scan of the log
  // has it, or when the scan and the window disagree on its text.
  function turnAnchor(promptAt: number): { seq: number; turn: SessionTurn } | null {
    const local = promptAt < 0 ? -1 : loadedTurnIndexOf(promptAt)
    if (promptAt >= 0 && local < 0) return null
    const seq = loadedTurnOffset.value + local
    const turn = sessionTurns.value[seq]
    if (!turn || (local >= 0 && promptTurns.value[local].label !== turn.label)) return null
    return { seq, turn }
  }

  // A loaded prompt the scan does not have yet (sent since) can still be
  // bookmarked: the click scans the log again first.
  function bookmarkState(promptAt: number, side: TurnSide): boolean | null {
    const anchor = turnAnchor(promptAt)
    if (anchor) return bookmarkByTurn.value.has(bookmarkKey(anchor.seq, side))
    return promptAt >= 0 ? false : null
  }

  async function toggleBookmark(runId: string, promptAt: number, side: TurnSide) {
    let anchor = turnAnchor(promptAt)
    if (!anchor) {
      await loadSessionTurns(runId)
      anchor = turnAnchor(promptAt)
    }
    if (currentRunId.value !== runId) return
    if (!anchor) {
      toast.error(t('run.bookmarks.unavailable'))
      return
    }
    const existing = bookmarkByTurn.value.get(bookmarkKey(anchor.seq, side))
    try {
      if (existing) await runsStore.deleteBookmark(existing.id)
      else await runsStore.addBookmark(runId, anchor.turn.offset, side, anchor.turn.label)
    } catch (e) {
      toast.error(String(e))
    }
    await loadBookmarks(runId)
  }

  // ── Quotes (UX-29) ───────────────────────────────────────────────────────────
  // The structured quotes of the run's sent prompts, as cards on their bubbles.
  // Each row says where its quoted message sits in the log now (Rust scans
  // for its id), so a card whose source a Rewind cut or a purge removed says
  // so instead of leading nowhere.
  const runQuotes = shallowRef<RunQuote[]>([])
  const quoteSets = computed(() => quoteSetViews(runQuotes.value))

  function shownHasMessage(id: string): boolean {
    return turnEntries.value.some((e) => e.kind === 'text' && e.mid === id)
  }

  async function loadQuotes(runId: string | null) {
    if (!runId) return
    try {
      const rows = await runsStore.listRunQuotes(runId)
      if (currentRunId.value !== runId) return
      // A source not in the log yet (still in the drain's buffer) but on screen
      // still leads somewhere.
      runQuotes.value = rows.map((q) =>
        q.kind === 'entry' && q.source_offset === null && q.source_entry_id && shownHasMessage(q.source_entry_id)
          ? { ...q, pending: true }
          : q,
      )
    } catch {
      if (currentRunId.value === runId) runQuotes.value = []
    }
  }

  /**
   * A prompt just sent with quotes: show its cards now, then read them back. A
   * resent edit (UX-30) can carry quotes of messages its rewind cut: those are
   * not on screen, so their cards say so at once.
   */
  function noteSentQuotes(runId: string, quoteSet: string, quotes: readonly DraftQuote[]) {
    if (currentRunId.value === runId && quotes.length) {
      const sent = pendingRunQuotes(runId, quoteSet, quotes).map((q) =>
        q.pending && !(q.source_entry_id && shownHasMessage(q.source_entry_id)) ? { ...q, pending: false } : q,
      )
      runQuotes.value = [...runQuotes.value, ...sent]
    }
  }

  watch(
    currentRunId,
    (id) => {
      runQuotes.value = []
      void loadQuotes(id)
    },
    { immediate: true },
  )
  // A turn's end flushes its messages to the log: what a card points at is there now.
  onUnmounted(live.onAnyDone((runId) => {
    if (runId === currentRunId.value) void loadQuotes(runId)
  }))

  // Opens the folded block a jumped-to message sits in (StreamLog).
  const quoteReveal = shallowRef<{ id: string; seq: number } | null>(null)
  let quoteRevealSeq = 0

  /**
   * Scroll to what a card quotes and highlight exactly that range. The history
   * pane loads the window holding its message first (one read, like a ⌘F hit);
   * the live pane shows what it has.
   */
  async function jumpToQuote(q: RunQuote) {
    const id = q.kind === 'entry' ? q.source_entry_id : null
    if (!id) return
    const runId = currentRunId.value
    if (!shownHasMessage(id) && isViewingHistory.value && historyHasStream.value && q.source_offset !== null) {
      const offset = q.source_offset
      firstLoadedSeq.value = promptsBefore(navTurns.value.map((t) => t.offset), offset)
      await jumpToHistoryOffset(offset)
    }
    if (currentRunId.value !== runId) return
    if (!shownHasMessage(id)) {
      toast.error(t(isViewingHistory.value || q.source_offset === null ? 'streamLog.quotes.sourceGone' : 'streamLog.quotes.notLoaded'))
      return
    }
    quoteReveal.value = { id, seq: ++quoteRevealSeq }
    await nextTick()
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
    const root = turnScrollEl.value
    if (!root) return
    const hosts = quoteHosts(root, id, q.block_index ?? 0)
    if (!hosts.length) return
    const start = q.start_offset ?? 0
    const end = q.end_offset ?? 0
    let host = hosts[0]
    let range: Range | null = null
    if (end > start) {
      for (const h of hosts) {
        range = quoteRangeIn(h, start, end, q.excerpt)
        if (range) {
          host = h
          break
        }
      }
    }
    // Reading back up: the next streamed chunk must not pull the reader down.
    if (!isViewingHistory.value) stickToBottom.value = false
    flashQuoteSource(host, range)
  }

  // ── Rewind (sessions.rewind) ─────────────────────────────────────────────────
  // Turns of the shown run whose workspace snapshot can still be restored.
  const snapshotTurns = shallowRef<Set<number>>(new Set())
  // `editResend`: the rewind of an edited message's Send (UX-30), told how it
  // ended through `settle` (true = rewound, the log on screen reloaded).
  const rewindTarget = ref<RewindTarget | null>(null)

  async function loadSnapshots(runId: string | null) {
    if (!runId || !features.isOn('sessions.rewind')) {
      snapshotTurns.value = new Set()
      return
    }
    try {
      const rows = await runsStore.listSnapshots(runId)
      if (currentRunId.value === runId) snapshotTurns.value = new Set(rows.filter((r) => r.status === 'ok').map((r) => r.turn))
    } catch {
      if (currentRunId.value === runId) snapshotTurns.value = new Set()
    }
  }

  // A turn's snapshot appears as it starts; reload when the run stops too, so the
  // last prompt's row is there by the time the rewind items show.
  watch(
    [currentRunId, currentStatus, () => features.isOn('sessions.rewind')],
    ([id]) => void loadSnapshots(id),
    { immediate: true },
  )

  /** The Rewind dialog closed without rewinding. */
  function closeRewind() {
    const target = rewindTarget.value
    rewindTarget.value = null
    target?.settle?.(false)
  }

  /** A rewind is done: from the dialog (`rewindTarget`), or straight from an edit's Send (`direct`). */
  async function onRewound(outcome: RewindOutcome, direct?: RewindTarget) {
    const target = direct ?? rewindTarget.value
    if (!direct) rewindTarget.value = null
    if (!target) return
    let applied = false
    try {
      await applyRewound(target, outcome)
      applied = true
    } finally {
      target.settle?.(applied && outcome.conversation)
    }
  }

  async function applyRewound(target: RewindTarget, outcome: RewindOutcome) {
    const { restored, deleted, failed } = outcome.files
    toast.success(t(outcome.conversation ? 'run.rewind.done' : 'run.rewind.doneFilesOnly', { restored, deleted }))
    if (failed.length) toast.error(t('run.rewind.failed', { files: failed.join(', ') }))
    if (currentRunId.value !== target.runId) return
    // The log was cut: drop the in-memory session and show the log from disk.
    if (outcome.conversation) {
      live.discard(target.runId)
      await loadRunLog(target.runId, { force: true })
      void loadSessionTurns(target.runId)
      // The cut dropped the bookmarks after it, and the messages some quotes point at.
      // Quotes are awaited: an edit's resend adds its own right after.
      void loadBookmarks(target.runId)
      await loadQuotes(target.runId)
    }
    void loadSnapshots(target.runId)
  }

  // ── Fork (UX-28) ─────────────────────────────────────────────────────────────
  // A new session with this one's conversation — all of it, or up to the end of
  // the turn starting at loaded entry `promptAt` (anchored like a bookmark, by
  // its prompt record's byte offset) — and its settings. Files are not touched.
  const forking = ref(false)

  async function forkRun(run: RunRecord, promptAt?: number) {
    if (forking.value) return
    let atOffset: number | undefined
    if (promptAt !== undefined) {
      let anchor = turnAnchor(promptAt)
      if (!anchor) {
        await loadSessionTurns(run.id)
        anchor = turnAnchor(promptAt)
      }
      if (!anchor) {
        toast.error(t('run.fork.errors.fork_point_unknown'))
        return
      }
      atOffset = anchor.turn.offset
    }
    const ok = await confirm({
      title: t(atOffset === undefined ? 'run.fork.confirmTitle' : 'run.fork.confirmTitleHere'),
      message: t(atOffset === undefined ? 'run.fork.confirmMessage' : 'run.fork.confirmMessageHere'),
      confirmLabel: t('run.fork.confirm'),
      cancelLabel: t('common.cancel'),
      variant: 'primary',
    })
    if (!ok) return
    forking.value = true
    try {
      const outcome = await runsStore.forkRun(run.id, atOffset)
      late.carryPicks(run.id, outcome.run.id)
      await runsStore.fetchRuns(projectId.value)
      toast.success(t(outcome.conversation ? 'run.fork.done' : 'run.fork.doneWithoutConversation'))
      // A session pop-out stays on its one session; the fork is in History.
      if (!isSessionWindow) await loadRunLog(outcome.run.id)
    } catch (e) {
      const key = forkErrorKey(e)
      toast.error(key ? t(key) : String(e))
    } finally {
      forking.value = false
    }
  }

  // ── Find (⌘F) ────────────────────────────────────────────────────────────────
  // The bar highlights matches in what is on screen. The history pane holds only
  // a window of the log, so it also lists hits from the WHOLE log; picking one
  // loads a window starting at that record, where the highlight lands. ⌘F comes
  // from App.vue's dispatcher (useFindShortcut).
  const runFind = usePageFind(turnScrollEl)
  const findOpen = runFind.open
  const logHits = shallowRef<RunLogHit[]>([])
  const logHitsTruncated = ref(false)
  const logSearching = ref(false)
  // Scanning the whole file costs more than a DOM pass: wait for a typing pause.
  const LOG_SEARCH_DEBOUNCE_MS = 250
  // Only the history pane of a stream log can load a window at a hit's offset.
  const logSearchActive = computed(
    () => runFind.open.value && !!runFind.query.value && isViewingHistory.value && historyHasStream.value,
  )
  let logSearchTimer: ReturnType<typeof setTimeout> | null = null
  // Bumped by every new search, so a slower stale answer is dropped.
  let logSearchSeq = 0

  watch([logSearchActive, runFind.query, runFind.caseSensitive, viewingLogRunId], () => {
    if (logSearchTimer) clearTimeout(logSearchTimer)
    const seq = ++logSearchSeq
    const runId = viewingLogRunId.value
    if (!logSearchActive.value || !runId) {
      logHits.value = []
      logHitsTruncated.value = false
      logSearching.value = false
      return
    }
    const query = runFind.query.value
    const caseSensitive = runFind.caseSensitive.value
    logSearching.value = true
    logSearchTimer = setTimeout(async () => {
      let hits: RunLogHit[] = []
      let truncated = false
      try {
        const r = await runsStore.searchRunLog(runId, query, caseSensitive)
        hits = r.hits
        truncated = r.truncated
      } catch {
        /* no list; the on-screen highlight still works */
      }
      if (seq !== logSearchSeq) return
      logHits.value = hits
      logHitsTruncated.value = truncated
      logSearching.value = false
    }, LOG_SEARCH_DEBOUNCE_MS)
  })

  async function jumpToLogHit(hit: RunLogHit) {
    if (!isViewingHistory.value) return
    firstLoadedSeq.value = promptsBefore(navTurns.value.map((t) => t.offset), hit.offset)
    await jumpToHistoryOffset(hit.offset)
    // The window now starts at the hit's record, so the first match on screen is it.
    runFind.refresh()
  }

  // A hit of the Search screen (SessionSearchView) opens its run here with
  // `?offset=&find=`: picked like a log hit above, its query put in the bar. A
  // run shown live has no log window to load, so only the highlight applies.
  async function openRouteSearchHit(hit: RouteHit) {
    runFind.query.value = hit.find
    runFind.openFind()
    if (historyHasStream.value) {
      await jumpToLogHit({ offset: hit.offset, snippet: '' })
      if (!sessionTurns.value.length) pendingSearchHit = { runId: viewingLogRunId.value, offset: hit.offset }
    }
    // Consumed: a later remount must not jump back to it.
    router.replace({ query: {} }).catch(() => {})
  }

  // On a fresh open the whole-session prompt scan can land after that jump, which
  // then counted no turns before the hit: count them once the scan is in.
  let pendingSearchHit: { runId: string | null; offset: number } | null = null
  watch(sessionTurns, (turns) => {
    const hit = pendingSearchHit
    if (!hit || !turns.length) return
    pendingSearchHit = null
    if (viewingLogRunId.value === hit.runId && historyCursor.value === hit.offset) {
      firstLoadedSeq.value = promptsBefore(turns.map((t) => t.offset), hit.offset)
    }
  })

  watch(currentRunId, () => {
    runFind.query.value = ''
    runFind.closeFind()
  })

  // Keep the live output pinned to the bottom as new entries/lines arrive for
  // the focused, running run — but only while the user is parked at the bottom
  // and not in the middle of selecting/clicking inside the output.
  // Whether we should currently keep the output pinned to the bottom: the run is
  // live, we're not in history view, the user is parked at the bottom, and isn't
  // mid-selection/click inside the output.
  function shouldPinToBottom() {
    return (
      currentStatus.value === 'running' &&
      !isViewingHistory.value &&
      stickToBottom.value &&
      !pointerDownInOutput.value &&
      !hasOutputSelection()
    )
  }

  // Pin in a single requestAnimationFrame — this runs AFTER Vue patches the stream
  // and the browser has computed layout, so we read `scrollHeight` exactly once
  // per frame instead of forcing a reflow on every streamed event (the old nested
  // nextTicks + repeated scrollHeight reads were the layout-thrash source). The
  // guards are re-checked inside the frame: a pointer interaction may begin after
  // the watcher fires but before this callback runs.
  function keepPinnedToBottom() {
    if (!shouldPinToBottom()) return
    requestAnimationFrame(() => {
      if (shouldPinToBottom() && outputEl.value) {
        outputEl.value.scrollTop = outputEl.value.scrollHeight
      }
    })
  }

  watch(
    () => [liveEntries.value.length, liveOutputLines.value.length, currentStatus.value] as const,
    keepPinnedToBottom
  )

  // When switching to another run's live output, jump to its latest output.
  watch(currentRunId, (id) => {
    // A run the reader left mid-way is restored by `loadRunLog` instead; pinning
    // here would let the next streamed chunk drag them away from it.
    const memo = scrollMemory.peek(id)
    stickToBottom.value = !memo
    if (!memo) scrollOutputToBottom()
    scrollActiveSessionIntoView()
    late.closeRunSettingsMenu()
    late.dismissSelection()
    // Keep the URL in sync with the selected run. Selecting a run from the History
    // list sets `currentRunId` locally but the route would otherwise stay at
    // `/projects/:projectId` with no runId — so a reload/remount (app backgrounding
    // for a notification, focus/resume, HMR) lands on the "no run selected" empty
    // state, and PermissionNotifier mistakes the on-screen run for a background one.
    // Mirroring it into the URL makes the selection survive remounts and lets
    // app-wide components tell which run is actually open.
    const current = typeof route.params.runId === 'string' ? route.params.runId : undefined
    if (current === (id ?? undefined)) return
    const target = id
      ? { name: 'project-run-detail', params: { projectId: projectId.value, runId: id } }
      : { name: 'project-run', params: { projectId: projectId.value } }
    router.replace(target).catch(() => {})
  })

  // Follow route-driven run changes (the OS-notification deep link in
  // PermissionNotifier, browser back/forward). onMounted handles the first load;
  // this keeps the view in sync for later route changes without a remount.
  watch(activeRunId, (id) => {
    if (id && id !== currentRunId.value) loadRunLog(id)
  })

  async function loadInputContent(runId: string) {
    if (inputContentRunId.value === runId && inputContent.value) return
    inputContentRunId.value = runId
    inputContentLoading.value = true
    inputContentError.value = null
    inputContent.value = ''
    try {
      inputContent.value = await runsStore.readRunInput(runId)
    } catch (e) {
      inputContentError.value = String(e)
    } finally {
      inputContentLoading.value = false
    }
  }

  // Clear the history (on-disk log) view; used when switching to live output.
  function clearHistoryView() {
    viewingLogRunId.value = null
    viewingLog.value = ''
    historyEntries.value = []
    historyHasStream.value = false
    historyToolIndex.clear()
    historyContextTokens.value = 0
    historyModel.value = null
    historyCursor.value = null
    historyHasMore.value = false
    historyBottomCursor.value = null
    historyHasMoreBelow.value = false
    historyFileSize.value = 0
    firstLoadedSeq.value = 0
    historyPreambleModel.value = null
    historyRevision.value = null
    historyToolEntries.value = []
    historyAwayFromBottom.value = false
  }

  async function loadRunLog(runId: string, opts: { preferDisk?: boolean; force?: boolean } = {}) {
    // Save where the reader was in the outgoing run while its DOM is still up.
    if (currentRunId.value !== runId) scrollMemory.flush()
    currentRunId.value = runId
    // Reflect this run's metadata so the chat composer can decide eligibility.
    const meta = runsStore.runs.find(r => r.id === runId)
    if (meta?.engine) {
      syncLoadedRunEngine(meta.engine)
    }
    // Kick off loading PR/Issue content in parallel so the Content tab is ready.
    // Standalone sessions have no input markdown — skip.
    if (meta?.run_type !== 'session') loadInputContent(runId).catch(() => {})

    // If we still hold a live in-memory session for this run (it's running, or it
    // finished during this app session), show that live instead of the on-disk
    // log — and re-attach listeners if it's still streaming.
    // Only `force` (the transcript actually changed on disk — the sessions:changed
    // watcher fired) overrides this for a finished session to re-read turns added
    // outside Puppeteer. A plain window refocus (preferDisk without force) keeps the
    // live view untouched so it doesn't blank + re-render (the "flash").
    const liveSession = live.get(runId)
    if (liveSession && !(opts.force && liveSession.status !== 'running')) {
      clearHistoryView()
      if (liveSession.status === 'running' && !live.isListening(runId)) {
        await live.startListening(runId, projectId.value)
      }
      if (!opts.preferDisk && !(await scrollMemory.restore(runId)) && currentRunId.value === runId) {
        stickToBottom.value = true
        scrollOutputToBottom()
      }
      return
    }

    // 'fetched' runs have no AI output yet — show Content first and let the
    // AI Result tab fall through to the "chưa run" empty state.
    if (meta?.status === 'fetched') {
      clearHistoryView()
      return
    }

    // On a focus refresh we're re-reading a log the user is already viewing, so
    // preserve their scroll position instead of yanking them to the bottom. Only
    // re-pin to the bottom if they were already near it.
    const isFocusRefresh = !!opts.preferDisk && viewingLogRunId.value === runId
    const prevScrollTop = historyEl.value?.scrollTop ?? 0
    const wasNearBottom = historyEl.value ? isNearBottom(historyEl.value) : true

    // Fresh open: clear immediately and show the loading animation. Focus refresh:
    // keep the current content on screen (no clear, no spinner) — we only touch
    // reactive state below IF the log actually changed, so an unchanged refocus
    // causes zero re-render (no flash).
    if (!isFocusRefresh) {
      historyEntries.value = []
      historyHasStream.value = false
      historyToolIndex.clear()
      historyContextTokens.value = 0
      historyModel.value = null
      historyCursor.value = null
      historyHasMore.value = false
      historyBottomCursor.value = null
      historyHasMoreBelow.value = false
      historyFileSize.value = 0
      firstLoadedSeq.value = 0
      historyPreambleModel.value = null
      historyRevision.value = null
      historyToolEntries.value = []
      historyAwayFromBottom.value = false
      viewingLog.value = ''
      historyLoading.value = true
    }
    viewingLogRunId.value = runId
    try {
      // A refocus on an unchanged log must cost nothing. The fingerprint is a
      // couple of stat() calls; before this, every alt-tab back into the app
      // re-read the entire file, re-encoded it as JSON across IPC and re-parsed
      // it, just to discover the bytes were identical.
      if (isFocusRefresh && historyRevision.value) {
        const rev = await runsStore.getRunLogRevision(runId)
        if (sameRunLogRevision(rev, historyRevision.value)) return
      }

      const page = await runsStore.getRunLogPage(runId, undefined, HISTORY_PAGE_INITIAL)
      // The user may have switched runs while this was in flight.
      if (viewingLogRunId.value !== runId) return
      const seedModel = modelFromPreamble(page.preamble)
      const parsed = parseStreamLogWindow(page.records, { seedModel })
      if (parsed.entries.length > 0) {
        historyToolIndex.clear()
        historyEntries.value = parsed.entries
        for (const [k, v] of parsed.toolIndex) historyToolIndex.set(k, v)
        historyHasStream.value = true
        historyContextTokens.value = parsed.contextTokens
        historyModel.value = parsed.model
        historyCursor.value = page.cursor
        historyHasMore.value = page.has_more
        // The initial window is the tail, so it already reaches the end of the log.
        historyBottomCursor.value = page.revision.size
        historyHasMoreBelow.value = false
        historyFileSize.value = page.revision.size
        firstLoadedSeq.value = 0
        historyPreambleModel.value = seedModel
        historyRevision.value = page.revision
        viewingLog.value = ''
        // Whole-run scan for the "files changed" list, off the critical path.
        void loadHistoryToolEntries(runId)
        const logSize = page.revision.size
        nextTick(async () => {
          if (!historyEl.value) return
          if (isFocusRefresh && !wasNearBottom) {
            // Keep the user where they were reading — don't jump on refocus.
            historyEl.value.scrollTop = prevScrollTop
            return
          }
          // Fresh load / switch run: back to where the reader left off, else the
          // end so the latest AI result is visible.
          if (!isFocusRefresh && (await scrollMemory.restore(runId, logSize))) return
          if (historyEl.value && viewingLogRunId.value === runId) {
            historyEl.value.scrollTop = historyEl.value.scrollHeight
          }
        })
      } else {
        // Nothing parsed as stream records. Either the run produced no output, or
        // this is a legacy/plain-markdown log — the only case that still needs the
        // whole file, so it's fetched here rather than on the hot path.
        const content = await runsStore.getRunLog(runId)
        if (viewingLogRunId.value !== runId) return
        if (content.trim()) {
          historyHasStream.value = false
          historyRevision.value = page.revision
          viewingLog.value = content
          if (!isFocusRefresh) void scrollMemory.restore(runId)
        } else {
          viewingLog.value = ''
          viewingLogRunId.value = null
        }
      }
    } catch (e) {
      viewingLog.value = String(e)
    } finally {
      historyLoading.value = false
    }
  }

  onUnmounted(() => {
    if (isResizing.value) stopResize()
    outputRO?.disconnect()
    outputRO = null
    contentRO?.disconnect()
    contentRO = null
    contentMO?.disconnect()
    contentMO = null
    if (scrollSettleFrame) {
      cancelAnimationFrame(scrollSettleFrame)
      scrollSettleFrame = 0
    }
    if (historyBottomFrame) {
      cancelAnimationFrame(historyBottomFrame)
      historyBottomFrame = 0
    }
    if (turnPositionFrame) {
      cancelAnimationFrame(turnPositionFrame)
      turnPositionFrame = 0
    }
    if (logSearchTimer) clearTimeout(logSearchTimer)
  })

  return {
    openSession, popoutSession, outputEl, historyEl, sessionListEl, viewingLogRunId, viewingLog,
    historyEntries, historyHasStream, historyLoading, historyContextTokens, historyModel,
    HISTORY_PAGE_STEP, historyHasMore, historyLoadingMore, historyHasMoreBelow, showEarlierHistory,
    showLaterHistory, inputContent, inputContentRunId, inputContentError, inputContentLoading,
    renderText, loadMarkdown, isViewingHistory, mentionedFileEntries, hasLiveOutput,
    splitContainerEl, isResizing, resultSplitEl, showContent, canToggleContent, contentVisible,
    showResizeHandle, noSession, contentWidth, resultWidth, startResize, sourceText, liveLegacyHtml,
    historyLegacyHtml, stickToBottom, scrollOutputToBottom, onOutputScroll, navTurns, scrollMemory,
    showTurnNav, activeTurnSeq, onTurnMenuToggle, turnPositionTracked, currentTurnSeq,
    turnAwayFromTop, scrollToFirstTurn, jumpToSessionTurn, prevTurn, nextTurn, onTurnNavKey,
    onOutputPointerDown, onWindowPointerUp, historyAwayFromBottom, scrollHistoryToLatest,
    onHistoryScroll, bookmarks, placedBookmarks, bookmarkState, toggleBookmark, snapshotTurns,
    rewindTarget, onRewound, closeRewind, promptNumber, forking, forkRun, runFind, findOpen, logHits,
    logHitsTruncated, logSearching,
    logSearchActive, jumpToLogHit, openRouteSearchHit, loadInputContent, clearHistoryView,
    loadRunLog, quoteSets, quoteReveal, jumpToQuote, loadQuotes, noteSentQuotes,
  }
}

export type RunLog = ReturnType<typeof useRunLog>
