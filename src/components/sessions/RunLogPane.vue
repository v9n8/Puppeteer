<script setup lang="ts">
// The run screen's main pane: the issue/PR Content column and the AI result
// (title row with the prompt navigator and bookmarks, ⌘F bar, history log or
// live output, jump buttons). State lives in useRunSession.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { openUrl } from '@tauri-apps/plugin-opener'
import {
  Terminal, FileText, MessageSquare, ChevronDown, ChevronUp, Loader2, Plus, Bookmark,
  ChevronsDownUp, ChevronsUpDown, GitFork,
} from 'lucide-vue-next'
import FindBar from '@/components/FindBar.vue'
import StreamLog from '@/components/StreamLog.vue'
import MentionedFiles from '@/components/MentionedFiles.vue'
import { Button, DropdownMenu, DropdownItem } from '@/components/ui'
import { turnCounterLabel } from '@/lib/turnPosition'
import { forkSource } from '@/lib/runFork'
import { vMermaid } from '@/lib/mermaid'
import { vCopyCode } from '@/lib/copyCode'
import { hrefFileRef, parseLineRef, decorateFileLinks } from '@/lib/fileLinks'
import { useRunSession } from './useRunSession'
import { useUILayoutStore } from '@/stores/uiLayout'

const { t } = useI18n()
const {
  currentRunId, currentRun, currentIsSession, liveEntries, liveHasStream, currentStatus, runLabel,
  outputEl, historyEl, historyEntries, historyHasStream, historyLoading, HISTORY_PAGE_STEP,
  historyHasMore, historyLoadingMore, historyHasMoreBelow, showEarlierHistory, showLaterHistory,
  inputContent, inputContentError, inputContentLoading, renderText, isViewingHistory,
  mentionedFileEntries, hasLiveOutput, splitContainerEl, isResizing, resultSplitEl, showContent,
  canToggleContent, contentVisible, showResizeHandle, noSession, contentWidth, resultWidth,
  startResize, liveLegacyHtml, historyLegacyHtml, stickToBottom, scrollOutputToBottom,
  onOutputScroll, navTurns, showTurnNav, activeTurnSeq, onTurnMenuToggle, turnPositionTracked,
  currentTurnSeq, turnAwayFromTop, scrollToFirstTurn, jumpToSessionTurn, prevTurn, nextTurn,
  onOutputPointerDown, historyAwayFromBottom, scrollHistoryToLatest, onHistoryScroll,
  placedBookmarks, runFind, findOpen, logHits, logHitsTruncated, logSearching, logSearchActive,
  jumpToLogHit, creatingSession, handleNewSession, fileMatcher, fileExists, pathChecker, fileListVersion, openFile, quoteSets, quoteReveal,
  jumpToQuote, runsStore, loadRunLog, isSessionWindow, session, project,
} = useRunSession()
// A fork names the session it came from, and opens it (not from a pop-out,
// which stays on its one session).
const forkedFrom = computed(() => forkSource(currentRun.value, runsStore.runs))
const forkSourceRun = computed(() => (forkedFrom.value && forkedFrom.value !== 'gone' ? forkedFrom.value : null))
const forkLinkable = computed(() => !!forkSourceRun.value && !isSessionWindow)
const forkedFromLabel = computed(() =>
  forkSourceRun.value ? t('run.fork.from', { title: runLabel(forkSourceRun.value) }) : t('run.fork.fromGone'),
)
function openForkSource() {
  if (forkLinkable.value && forkSourceRun.value) void loadRunLog(forkSourceRun.value.id)
}
const uiLayout = useUILayoutStore()

// Directive for RunView's own markdown containers (issue/PR Content, legacy
// HTML). Marks inline `<code>` that names a project file as a clickable link.
const vFileLinks = {
  mounted: (el: HTMLElement) => decorateFileLinks(el, fileMatcher),
  updated: (el: HTMLElement) => decorateFileLinks(el, fileMatcher),
}
function onProseClick(e: MouseEvent) {
  const target = e.target as HTMLElement
  const code = target.closest('code.file-link') as HTMLElement | null
  const path = code?.getAttribute('data-file-path')
  if (path) { e.preventDefault(); openFile(path, parseLineRef(code?.textContent ?? '')); return }
  // Markdown link: prevent the webview from navigating (relative hrefs blank
  // the SPA). Open project files in the viewer, external URLs in the browser.
  const anchor = target.closest('a') as HTMLAnchorElement | null
  if (anchor) {
    e.preventDefault()
    const href = anchor.getAttribute('href') ?? ''
    if (!href) return
    if (/^(https?:|mailto:)/i.test(href)) { onOpenUrl(href); return }
    const ref = hrefFileRef(href)
    if (!ref) return
    const fp = fileMatcher(ref.path)
    if (fp) { openFile(fp, ref.line); return }
    // Not in the project list: open it only when it is a real file.
    void fileExists(ref.path).then((ok) => { if (ok) openFile(ref.path, ref.line) })
  }
}

function onOpenUrl(url: string) {
  openUrl(url).catch(() => { /* opener unavailable */ })
}
</script>

<template>
  <!-- No sessions yet: hide the Content / AI Result panels entirely and
       show a single centered empty state. -->
  <div v-if="noSession" class="flex-1 flex items-center justify-center p-6">
    <div class="text-center max-w-xs">
      <MessageSquare class="h-10 w-10 text-foreground/20 mx-auto mb-4" :stroke-width="1" />
      <p class="text-sm font-medium text-foreground/70 mb-1.5">{{ t('run.noSessionsYet') }}</p>
      <p class="text-xs text-foreground/40 leading-relaxed mb-4">
        {{ t('run.createNewSessionHint') }}
      </p>
      <Button :disabled="creatingSession" @click="handleNewSession">
        <Plus class="h-3.5 w-3.5" :stroke-width="2" />
        {{ creatingSession ? t('run.creating') : t('run.newSession') }}
      </Button>
    </div>
  </div>

  <!-- Split container -->
  <div
    v-else
    ref="splitContainerEl"
    class="flex-1 flex overflow-hidden min-h-0"
  >
    <!-- Content column (hidden for sessions or when toggled off) -->
    <div
      v-if="contentVisible"
      class="flex flex-col overflow-hidden bg-background min-w-0 min-h-0"
      :style="{ width: contentWidth }"
    >
      <!-- h-8: same row height as the rail tab bar and the AI Result
           header, so all three border lines meet up. -->
      <div class="flex items-center gap-2 px-3 h-8 bg-card border-b border-border shrink-0">
        <FileText class="h-3.5 w-3.5 text-foreground/40" :stroke-width="1.5" />
        <span class="text-2xs font-mono text-foreground/60">{{ t('run.content') }}</span>
      </div>
      <div class="flex-1 min-h-0 overflow-auto p-4">
        <template v-if="!currentRunId">
          <div class="flex items-center justify-center h-full">
            <div class="text-center">
              <FileText class="h-8 w-8 text-foreground/20 mx-auto mb-3" :stroke-width="1" />
              <p class="text-xs text-foreground/30 font-mono">{{ t('run.fetchToBegin') }}</p>
            </div>
          </div>
        </template>
        <template v-else-if="inputContentLoading">
          <div class="flex items-center gap-2 text-xs text-muted-foreground">
            <Loader2 class="h-3.5 w-3.5 animate-spin text-primary" :stroke-width="2" />
            <span>{{ t('run.loadingContent') }}</span>
          </div>
        </template>
        <template v-else-if="inputContentError">
          <div class="text-xs text-destructive">
            <p class="font-medium mb-1">{{ t('run.couldNotLoadContent') }}</p>
            <p class="text-foreground/60">{{ inputContentError }}</p>
          </div>
        </template>
        <template v-else-if="inputContent">
          <div
            v-file-links
            v-mermaid
            v-copy-code
            v-html="renderText(inputContent)"
            class="markdown-output text-sm leading-relaxed text-foreground"
            @click="onProseClick"
          />
        </template>
        <template v-else>
          <p class="text-xs text-foreground/40">{{ t('run.noContentAvailable') }}</p>
        </template>
      </div>
    </div>

    <!-- Resize handle (only when both panels are visible) -->
    <div
      v-if="showResizeHandle"
      class="group relative shrink-0 w-px bg-border cursor-col-resize select-none"
      :class="{ 'bg-primary/60': isResizing }"
      @mousedown="startResize"
    >
      <!-- Wider hover hit area for easier grabbing. Extends only to the
           right so it never overlaps the Content column's scrollbar. -->
      <div
        class="absolute inset-y-0 left-0 -right-2.5 z-10 transition-colors group-hover:bg-primary/30"
        :class="{ 'bg-primary/40': isResizing }"
      />
    </div>

    <!-- AI Result column (always shown; full width when Content is hidden) -->
    <div
      ref="resultSplitEl"
      class="relative flex overflow-hidden bg-background min-w-0 min-h-0"
      :style="{ width: resultWidth }"
    >
      <!-- AI result main content (left; always flex-1 so it fills the
           space the moment the question panel appears/disappears — no
           intermediate frame where the column is stuck at a fixed width). -->
      <div class="relative flex-1 flex flex-col overflow-hidden min-w-0 min-h-0">
      <div class="bg-card border-b border-border shrink-0">
        <!-- Title row: shows the current run / session title. Fixed h-8
             (= rail tab bar height) so the header never grows when the
             mentioned-files / re-dock controls appear. -->
        <div class="flex items-center gap-2 px-3 h-8">
          <MessageSquare class="h-3.5 w-3.5 text-foreground/40 shrink-0" :stroke-width="1.5" />
          <span class="text-xs font-medium text-foreground/90 truncate">
            {{ currentRun ? runLabel(currentRun) : t('run.noRunSelected') }}
          </span>
          <component
            :is="forkLinkable ? 'button' : 'span'"
            v-if="forkedFrom"
            :type="forkLinkable ? 'button' : undefined"
            class="flex min-w-0 items-center gap-1 text-2xs text-muted-foreground"
            :class="{ 'cursor-pointer hover:text-foreground hover:underline': forkLinkable }"
            :title="forkSourceRun ? t('run.fork.fromTitle') : t('run.fork.fromGoneTitle')"
            @click="openForkSource"
          >
            <GitFork class="h-3 w-3 shrink-0" :stroke-width="1.75" />
            <span class="truncate">{{ forkedFromLabel }}</span>
          </component>
          <div class="ml-auto flex items-center gap-1.5 shrink-0">
            <!-- Prompt navigator: step between the user's own messages or
                 pick one from the list. Lives in the toolbar (not floating
                 over the chat) so it never covers the conversation. Only
                 worth showing once there is more than one prompt. -->
            <div
              v-if="showTurnNav"
              class="flex items-center h-6 rounded border border-border text-2xs font-mono text-foreground/70"
            >
              <button
                type="button"
                class="flex items-center h-full rounded-l px-1 transition-colors hover:bg-accent/60 hover:text-foreground cursor-pointer"
                :title="t('run.prevPrompt')"
                @click="prevTurn"
              >
                <ChevronUp class="h-3.5 w-3.5" :stroke-width="2" />
              </button>
              <DropdownMenu align="right" @update:open="onTurnMenuToggle">
                <template #trigger>
                  <button
                    type="button"
                    class="flex items-center gap-1 h-full border-x border-border px-1.5 transition-colors hover:bg-accent/60 hover:text-foreground cursor-pointer"
                    :title="t('run.promptList')"
                  >
                    <MessageSquare class="h-3.5 w-3.5" :stroke-width="1.75" />
                    <span class="tabular-nums">{{ turnCounterLabel(currentTurnSeq, navTurns.length) }}</span>
                  </button>
                </template>
                <div class="max-h-96 w-[24rem] max-w-[80vw] overflow-y-auto">
                  <div class="px-2.5 pt-1 pb-1.5 text-2xs font-medium uppercase tracking-wide text-foreground/40">
                    {{ t('run.promptList') }}
                  </div>
                  <DropdownItem
                    v-for="(turn, ti) in navTurns"
                    :key="ti"
                    :variant="ti === activeTurnSeq ? 'primary' : 'default'"
                    class="!items-start"
                    @click="jumpToSessionTurn(ti, true)"
                  >
                    <span
                      class="mt-px flex h-4 min-w-4 shrink-0 items-center justify-center rounded px-1 text-2xs font-mono tabular-nums"
                      :class="ti === activeTurnSeq ? 'bg-primary/15 text-primary' : 'bg-muted text-foreground/45'"
                    >{{ ti + 1 }}</span>
                    <span
                      class="min-w-0 flex-1 whitespace-normal break-words leading-snug line-clamp-2"
                      :class="turn.label ? '' : 'italic text-foreground/40'"
                    >{{ turn.label || t('run.promptNoText') }}</span>
                  </DropdownItem>
                </div>
              </DropdownMenu>
              <button
                type="button"
                class="flex items-center h-full rounded-r px-1 transition-colors hover:bg-accent/60 hover:text-foreground cursor-pointer"
                :title="t('run.nextPrompt')"
                @click="nextTurn"
              >
                <ChevronDown class="h-3.5 w-3.5" :stroke-width="2" />
              </button>
            </div>
            <!-- Bookmarked turns of this session; picking one jumps there
                 like the prompt list does (a window read at its offset). -->
            <DropdownMenu
              v-if="placedBookmarks.length && (isViewingHistory ? historyHasStream : liveHasStream)"
              align="right"
            >
              <template #trigger>
                <button
                  type="button"
                  class="flex items-center gap-1 h-6 rounded border border-border px-1.5 text-2xs font-mono text-foreground/70 transition-colors hover:bg-accent/60 hover:text-foreground cursor-pointer"
                  :title="t('run.bookmarks.title')"
                >
                  <Bookmark class="h-3.5 w-3.5" :stroke-width="1.75" />
                  <span class="tabular-nums">{{ placedBookmarks.length }}</span>
                </button>
              </template>
              <div class="max-h-96 w-[24rem] max-w-[80vw] overflow-y-auto">
                <div class="px-2.5 pt-1 pb-1.5 text-2xs font-medium uppercase tracking-wide text-foreground/40">
                  {{ t('run.bookmarks.title') }}
                </div>
                <DropdownItem
                  v-for="placed in placedBookmarks"
                  :key="placed.bookmark.id"
                  class="!items-start"
                  @click="jumpToSessionTurn(placed.seq, true)"
                >
                  <span class="mt-px flex h-4 min-w-4 shrink-0 items-center justify-center rounded bg-muted px-1 text-2xs font-mono tabular-nums text-foreground/45">{{ placed.seq + 1 }}</span>
                  <span
                    class="min-w-0 flex-1 whitespace-normal break-words leading-snug line-clamp-2"
                    :class="placed.bookmark.label ? '' : 'italic text-foreground/40'"
                  >{{ placed.bookmark.label || t('run.promptNoText') }}</span>
                  <span class="mt-px shrink-0 text-2xs text-foreground/45">{{ t(`run.bookmarks.${placed.bookmark.side}`) }}</span>
                </DropdownItem>
              </div>
            </DropdownMenu>
            <!-- Show the steps of every turn, or fold finished turns (the
                 default). Remembered for this viewer. -->
            <button
              v-if="isViewingHistory ? historyHasStream : liveHasStream"
              type="button"
              class="flex items-center h-6 rounded border border-border px-1 text-foreground/60 transition-colors hover:bg-accent/60 hover:text-foreground cursor-pointer"
              :class="{ 'bg-primary/15 text-primary border-primary/30': uiLayout.stepsExpanded }"
              :title="uiLayout.stepsExpanded ? t('streamLog.activity.collapseAll') : t('streamLog.activity.expandAll')"
              :aria-pressed="uiLayout.stepsExpanded"
              @click="uiLayout.setStepsExpanded(!uiLayout.stepsExpanded)"
            >
              <component :is="uiLayout.stepsExpanded ? ChevronsDownUp : ChevronsUpDown" class="h-3.5 w-3.5" :stroke-width="1.75" />
            </button>
            <!-- Issue / PR runs only: opens the fetched Content as a left
                 split column. Sessions have no Content to show. -->
            <button
              v-if="canToggleContent"
              type="button"
              class="flex items-center gap-1 rounded h-6 px-1.5 text-2xs font-medium transition-colors cursor-pointer"
              :class="contentVisible ? 'bg-primary/15 text-primary' : 'text-foreground/50 hover:text-foreground/80 hover:bg-accent/60'"
              :title="t('run.toggleContentPanel')"
              :aria-pressed="contentVisible"
              @click="showContent = !showContent"
            >
              <FileText class="h-3.5 w-3.5" :stroke-width="1.75" />
              {{ t('run.content') }}
            </button>
            <MentionedFiles :entries="mentionedFileEntries" @open-file="openFile" />
          </div>
        </div>
      </div>

      <!-- ⌘F bar; on the history pane, with the hits of the whole log. -->
      <div
        v-if="findOpen"
        class="absolute top-10 right-4 z-20 flex w-80 max-w-[calc(100%-2rem)] flex-col items-end gap-1"
      >
        <FindBar :find="runFind" />
        <div
          v-if="logSearchActive"
          class="w-full rounded-md border border-border bg-card text-xs shadow-lg shadow-black/30"
        >
          <div class="border-b border-border px-2.5 py-1.5 text-2xs text-foreground/50">
            <template v-if="logSearching">{{ t('find.searchingSession') }}</template>
            <template v-else-if="!logHits.length">{{ t('find.noSessionMatches') }}</template>
            <template v-else-if="logHitsTruncated">{{ t('find.sessionMatchesFirst', { count: logHits.length }) }}</template>
            <template v-else>{{ t('find.sessionMatches', logHits.length) }}</template>
          </div>
          <div v-if="logHits.length" class="max-h-64 overflow-y-auto py-1">
            <button
              v-for="hit in logHits"
              :key="hit.offset"
              type="button"
              class="w-full px-2.5 py-1 text-left text-foreground/80 break-words line-clamp-2 transition-colors hover:bg-accent/60 hover:text-foreground cursor-pointer"
              @click="jumpToLogHit(hit)"
            >
              {{ hit.snippet }}
            </button>
          </div>
        </div>
      </div>

      <!-- Log viewer (selected history run) -->
      <div
        v-if="isViewingHistory"
        ref="historyEl"
        class="chat-text flex-1 min-h-0 overflow-auto p-4"
        @scroll="onHistoryScroll"
      >
        <!-- Loading animation while the log is read + parsed from disk. -->
        <div
          v-if="historyLoading"
          class="flex h-full flex-col items-center justify-center gap-3 text-muted-foreground"
        >
          <Loader2 class="h-6 w-6 animate-spin text-primary" :stroke-width="2" />
          <span class="text-xs">{{ t('run.loadingLog') }}</span>
        </div>
        <template v-else-if="historyHasStream">
          <!-- Fetch the next window of older records on demand. Opening a
               run only reads its tail, so the first paint stays fast no
               matter how long the conversation got. -->
          <div v-if="historyHasMore" class="search-skip mb-3 flex justify-center">
            <button
              type="button"
              class="rounded-full border border-border bg-muted/60 px-3 py-1 text-2xs text-foreground/70 hover:bg-accent/60 transition-colors cursor-pointer disabled:opacity-50"
              :disabled="historyLoadingMore"
              @click="showEarlierHistory"
            >
              {{ historyLoadingMore ? t('common.loading') : t('run.showEarlier', { count: HISTORY_PAGE_STEP }) }}
            </button>
          </div>
          <StreamLog
            :entries="historyEntries"
            :running="false"
            :render-text="renderText"
            :file-matcher="fileMatcher"
            :file-exists="fileExists"
            :file-list-version="fileListVersion"
            :path-checker="pathChecker"
            :project-path="project?.path"
            group-steps
            :steps-expanded="uiLayout.stepsExpanded"
            :find-query="findOpen ? runFind.query.value : ''"
            :find-case-sensitive="runFind.caseSensitive.value"
            :quote-sets="quoteSets"
            :quote-reveal="quoteReveal"
            @open-file="openFile"
            @open-url="onOpenUrl"
            @quote-jump="jumpToQuote"
          />
          <!-- Newer records below the loaded window (only after a jump into
               the middle of the log). Scrolling down loads them too. -->
          <div v-if="historyHasMoreBelow" class="search-skip mt-3 flex justify-center">
            <button
              type="button"
              class="rounded-full border border-border bg-muted/60 px-3 py-1 text-2xs text-foreground/70 hover:bg-accent/60 transition-colors cursor-pointer disabled:opacity-50"
              :disabled="historyLoadingMore"
              @click="showLaterHistory"
            >
              {{ historyLoadingMore ? t('common.loading') : t('run.showLater', { count: HISTORY_PAGE_STEP }) }}
            </button>
          </div>
        </template>
        <div
          v-else
          v-file-links
          v-mermaid
          v-copy-code
          v-html="historyLegacyHtml"
          class="markdown-output text-sm leading-relaxed text-foreground"
          @click="onProseClick"
        />
      </div>

      <!-- Live output -->
      <div
        v-else-if="hasLiveOutput"
        ref="outputEl"
        class="chat-text flex-1 min-h-0 overflow-auto p-4"
        @scroll="onOutputScroll"
        @pointerdown="onOutputPointerDown"
      >
        <StreamLog
          v-if="liveHasStream"
          :entries="liveEntries"
          :running="currentStatus === 'running'"
          :live-status="session?.liveStatus ?? null"
          :render-text="renderText"
          :file-matcher="fileMatcher"
          :file-exists="fileExists"
          :file-list-version="fileListVersion"
          :path-checker="pathChecker"
          :project-path="project?.path"
          group-steps
          :steps-expanded="uiLayout.stepsExpanded"
          :find-query="findOpen ? runFind.query.value : ''"
          :find-case-sensitive="runFind.caseSensitive.value"
          :quote-sets="quoteSets"
          :quote-reveal="quoteReveal"
          @open-file="openFile"
          @open-url="onOpenUrl"
          @quote-jump="jumpToQuote"
        />
        <template v-else>
          <div
            v-file-links
            v-mermaid
            v-copy-code
            v-html="liveLegacyHtml"
            class="markdown-output text-sm leading-relaxed text-foreground"
            @click="onProseClick"
          />
          <div v-if="currentStatus === 'running'" class="text-emerald-500 dark:text-emerald-400 animate-pulse mt-1 select-none font-mono text-xs">▌</div>
        </template>
      </div>

      <!-- Empty state -->
      <div v-else class="flex-1 flex items-center justify-center p-4">
        <div class="text-center max-w-xs">
          <Terminal class="h-8 w-8 text-foreground/20 mx-auto mb-3" :stroke-width="1" />
          <template v-if="currentRunId && currentStatus === 'fetched'">
            <p class="text-xs font-medium text-warning/80 font-mono mb-1">{{ t('run.notRun') }}</p>
            <!-- A fresh session needs a typed goal; an issue/PR can Run on its
                 default prompt. -->
            <p class="text-2xs text-foreground/40 leading-relaxed">
              {{ currentIsSession ? t('run.notRunSessionHint') : t('run.notRunHint') }}
            </p>
          </template>
          <p v-else class="text-xs text-foreground/30 font-mono">
            {{ t('run.fetchIssueOrPrThenRun') }}
          </p>
        </div>
      </div>

      <!-- Jump-to-top / jump-to-latest buttons: "Top" once the reader
           has left the start of the log, "Latest" once they have scrolled
           up away from the bottom. The prompt navigator now lives in the
           toolbar, so these always sit at the bottom. -->
      <div class="absolute bottom-4 right-4 z-10 flex items-center gap-2">
        <button
          v-if="turnPositionTracked && turnAwayFromTop"
          class="flex items-center gap-1 rounded-full border border-border bg-card px-3 py-1.5 text-2xs font-mono text-foreground/80 shadow-md transition-colors hover:bg-card hover:text-foreground cursor-pointer"
          :title="t('run.scrollToTop')"
          @click="scrollToFirstTurn"
        >
          <ChevronUp class="h-3.5 w-3.5" :stroke-width="2" />
          {{ t('run.top') }}
        </button>
        <button
          v-if="hasLiveOutput && !isViewingHistory && !stickToBottom"
          class="flex items-center gap-1 rounded-full border border-border bg-card px-3 py-1.5 text-2xs font-mono text-foreground/80 shadow-md transition-colors hover:bg-card hover:text-foreground cursor-pointer"
          :title="t('run.scrollToLatest')"
          @click="stickToBottom = true; scrollOutputToBottom()"
        >
          <ChevronDown class="h-3.5 w-3.5" :stroke-width="2" />
          {{ t('run.latest') }}
        </button>
        <button
          v-else-if="isViewingHistory && historyHasStream && !historyLoading && (historyAwayFromBottom || historyHasMoreBelow)"
          class="flex items-center gap-1 rounded-full border border-border bg-card px-3 py-1.5 text-2xs font-mono text-foreground/80 shadow-md transition-colors hover:bg-card hover:text-foreground cursor-pointer"
          :title="t('run.scrollToLatest')"
          @click="scrollHistoryToLatest"
        >
          <ChevronDown class="h-3.5 w-3.5" :stroke-width="2" />
          {{ t('run.latest') }}
        </button>
      </div>
      </div>

      <!-- The permission / question prompt is no longer an inline drawer:
           it pops out into its own window the instant a request arrives
           (see the openPermissionPopout watch), so the chat is never
           covered. -->
    </div>
  </div>
</template>
