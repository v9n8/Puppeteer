// RT-07: "Show earlier" for the live pane. A live session holds what this
// window received since it started listening; when that is not the whole log
// (seeded from a history window on resume, or opened mid-run after a prompt
// sent from another window) the older part comes from the log on disk, a page
// at a time, like the history pane. Rules: lib/livePaging. Created by
// useRunLog, which owns the history state a resume seeds from.

import { computed, nextTick, ref, type Ref } from 'vue'
import { modelFromPreamble, parseStreamLogWindow, type StreamEntry } from '@/lib/streamEvents'
import { mergeTailWithLive, pagingFromHistory, prependPage, type LivePaging } from '@/lib/livePaging'
import type { useLiveRunsStore, LiveSession } from '@/stores/liveRuns'
import type { useRunsStore } from '@/stores/runs'

/** Pages read back before giving up on placing a live copy in its log. */
const MAX_TAIL_PAGES = 40

export interface LivePagingDeps {
  runsStore: ReturnType<typeof useRunsStore>
  live: ReturnType<typeof useLiveRunsStore>
  currentRunId: Ref<string | null>
  outputEl: Ref<HTMLDivElement | null>
  stickToBottom: Ref<boolean>
  scrollOutputToBottom: () => void
  pageInitial: number
  pageStep: number
  /** The history window on screen, for seeding a resumed run's live copy. */
  history: {
    runId: Ref<string | null>
    hasStream: Ref<boolean>
    entries: Ref<StreamEntry[]>
    cursor: Ref<number | null>
    hasMore: Ref<boolean>
    hasMoreBelow: Ref<boolean>
    preambleModel: Ref<string | null>
    contextTokens: Ref<number>
  }
}

export function useLivePaging(d: LivePagingDeps) {
  const loading = ref(new Set<string>())
  const liveSession = computed<LiveSession | undefined>(() =>
    d.currentRunId.value ? d.live.sessions.get(d.currentRunId.value) : undefined,
  )
  const liveHasMore = computed(() => !!liveSession.value?.paging?.hasMore)
  /** Byte offset the live copy's loaded part starts at (null: not placed yet). */
  const liveCursor = computed(() => liveSession.value?.paging?.cursor ?? null)
  // The read in flight per run, so a caller paging back (RT-08) can wait for it.
  const inflight = new Map<string, Promise<void>>()
  function track(runId: string, p: Promise<void>): Promise<void> {
    inflight.set(runId, p)
    return p.finally(() => {
      if (inflight.get(runId) === p) inflight.delete(runId)
    })
  }
  const liveLoadingMore = computed(() => !!d.currentRunId.value && loading.value.has(d.currentRunId.value))

  function setLoading(runId: string, on: boolean) {
    const next = new Set(loading.value)
    if (on) next.add(runId)
    else next.delete(runId)
    loading.value = next
  }

  // Older entries push the content down: keep the reader's place by moving the
  // scroll by exactly the height they added, unless the pane follows the end.
  async function keepPlace(runId: string, change: () => void) {
    const el = d.outputEl.value
    const beforeHeight = el?.scrollHeight ?? 0
    const beforeTop = el?.scrollTop ?? 0
    change()
    if (d.currentRunId.value !== runId) return
    await nextTick()
    if (d.stickToBottom.value) {
      d.scrollOutputToBottom()
      return
    }
    requestAnimationFrame(() => {
      const e = d.outputEl.value
      if (e) e.scrollTop = beforeTop + (e.scrollHeight - beforeHeight)
    })
  }

  /**
   * Place a live copy of unknown start (`paging` null) in its log: read the
   * log's tail and join it in front of the copy, reading further back while
   * the copy holds the whole page. Once per copy.
   */
  function joinLiveTail(runId: string): Promise<void> {
    const s = d.live.sessions.get(runId)
    if (!s || s.paging !== null || loading.value.has(runId)) return Promise.resolve()
    return track(runId, readTail(runId, s))
  }

  async function readTail(runId: string, s: LiveSession) {
    setLoading(runId, true)
    try {
      let before: number | undefined
      let seedModel: string | null = null
      let paging: LivePaging | null = null
      let merged: StreamEntry[] | null = null
      let tailTokens = 0
      for (let i = 0; i < MAX_TAIL_PAGES; i++) {
        const page = await d.runsStore.getRunLogPage(runId, before, i === 0 ? d.pageInitial : d.pageStep)
        if (i === 0) seedModel = modelFromPreamble(page.preamble)
        const parsed = parseStreamLogWindow(page.records, { seedModel })
        const m = mergeTailWithLive(parsed.entries, s.entries)
        if (i === 0) tailTokens = parsed.contextTokens
        if (m.kind === 'merged') {
          merged = m.entries
          paging = { cursor: page.cursor, hasMore: page.has_more, seedModel }
          break
        }
        // The copy holds this whole page: it starts further back.
        if (!page.has_more) {
          paging = { cursor: 0, hasMore: false, seedModel }
          break
        }
        before = page.cursor
      }
      if (s.paging !== null) return
      // Not placed within the read limit: keep the copy as it is, without a
      // "Show earlier" that could not line up.
      s.paging = paging ?? { cursor: 0, hasMore: false, seedModel }
      // RT-09: a copy with no reply yet takes the figure from the log's tail.
      if (merged && !s.sawAssistantUsage && tailTokens > 0) {
        s.contextTokens = tailTokens
        s.sawAssistantUsage = true
      }
      if (merged) {
        const entries = merged
        await keepPlace(runId, () => d.live.replaceEntries(runId, entries))
      }
    } catch {
      /* the pane keeps what it shows; the next open tries again */
    } finally {
      setLoading(runId, false)
    }
  }

  /**
   * Load the page of the log just older than the live pane's first entry. A
   * read already in flight is waited for instead (its page is the progress).
   */
  async function showEarlierLive() {
    const runId = d.currentRunId.value
    const s = runId ? d.live.sessions.get(runId) : undefined
    if (!runId || !s) return
    const pending = inflight.get(runId)
    if (pending) return pending
    if (!s.paging?.hasMore) return
    return track(runId, readEarlier(runId, s, s.paging))
  }

  async function readEarlier(runId: string, s: LiveSession, paging: LivePaging) {
    setLoading(runId, true)
    try {
      const page = await d.runsStore.getRunLogPage(runId, paging.cursor, d.pageStep)
      if (s.paging !== paging) return
      const parsed = parseStreamLogWindow(page.records, { seedModel: paging.seedModel })
      s.paging = { cursor: page.cursor, hasMore: page.has_more, seedModel: paging.seedModel }
      await keepPlace(runId, () => d.live.replaceEntries(runId, prependPage(s.entries, parsed.entries)))
    } catch {
      /* leave what is loaded on screen */
    } finally {
      setLoading(runId, false)
    }
  }

  /**
   * A run on screen goes live (resume): carry the history window into its
   * live copy with the window's paging, so the older part stays reachable.
   * A window that does not reach the end of the log cannot seed (it would
   * leave a hole): the copy then joins the disk tail instead.
   */
  function seedLiveFromHistory(runId: string, s: LiveSession) {
    const h = d.history
    if (s.entries.length === 0 && h.runId.value === runId && h.hasStream.value) {
      const paging = pagingFromHistory({
        cursor: h.cursor.value,
        hasMore: h.hasMore.value,
        hasMoreBelow: h.hasMoreBelow.value,
        seedModel: h.preambleModel.value,
      })
      if (paging) {
        s.entries.push(...h.entries.value)
        s.hasStreamEvents = true
        for (const e of h.entries.value) {
          if (e.kind === 'system' && e.sessionId) s.sessionId = e.sessionId
          if (e.kind === 'system' && e.slashCommands?.length) s.slashCommands = e.slashCommands
        }
        d.live.replaceEntries(runId, s.entries)
        s.paging = paging
        // RT-09: the meter keeps the figure the log gave until the next reply.
        if (!s.sawAssistantUsage && h.contextTokens.value > 0) {
          s.contextTokens = h.contextTokens.value
          s.sawAssistantUsage = true
        }
        return
      }
    }
    if (s.paging === null) void joinLiveTail(runId)
  }

  return { liveHasMore, liveCursor, liveLoadingMore, showEarlierLive, joinLiveTail, seedLiveFromHistory }
}

export type LivePagingApi = ReturnType<typeof useLivePaging>
