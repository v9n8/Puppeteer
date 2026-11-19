<script setup lang="ts">
// The Session area's tab bar (V3-24), under the header: "Conversation" first,
// then the files this Session opened and the "Changed files" tab (V3-26); pinned
// ones come right after it. Right-click a file tab for pin / close / split, then
// the file's own rows (files/FileTabMenuItems). When the tabs overflow, the bar
// scrolls and "+N ▾" lists the ones out of view. Tabs per Session live in
// stores/sessionTabs; the rules in lib/sessionTabs.
// Split view (V3-27): `group` 'all' is the one bar; 'left' only Conversation;
// 'right' the file tabs with "Unsplit". 'all' ends with the "Split" button (icon only).
// File tabs drag to reorder (UR-33) within their group, pinned or not.
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, type Component } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  ChevronDown, Columns2, File, FileCode, FileDiff, FileImage, FileJson, FileText, MessageSquare, PanelRight,
  PanelRightClose, Pin, PinOff, X,
} from 'lucide-vue-next'
import { ContextMenu, DropdownItem, DropdownMenu, DropdownSeparator } from '@/components/ui'
import {
  CONVERSATION, fileTabIcon, hiddenTabIndexes, orderedTabs, splitBlockedReason, tabLabel,
  type FileTab, type FileTabIcon, type SessionTab, type SessionTabId,
} from '@/lib/sessionTabs'
import { useSessionTabsStore } from '@/stores/sessionTabs'
import { requestCompare } from '@/composables/useFileCompare'
import { useDragSort } from '@/composables/useDragSort'
import FileTabMenuItems from '@/components/files/FileTabMenuItems.vue'
import { useChangedFilesCount } from './useChangedFiles'
import SessionTabTools from './SessionTabTools.vue'
import { useRunSession } from './useRunSession'

const props = withDefaults(defineProps<{ group?: 'all' | 'left' | 'right' }>(), { group: 'all' })
const { t } = useI18n()
const { currentRunId, project, uiLayout } = useRunSession()
const tabsStore = useSessionTabsStore()

const ICONS: Record<FileTabIcon, Component> = { code: FileCode, json: FileJson, image: FileImage, text: FileText, file: File }

const runId = computed(() => currentRunId.value ?? '')
const state = computed(() => tabsStore.stateFor(runId.value))
const tabs = computed(() => orderedTabs(state.value))
const showConversation = computed(() => props.group !== 'right')
const shownTabs = computed(() => (props.group === 'left' ? [] : tabs.value))
// The tab selected in this bar: the right side's own in the right bar.
const activeId = computed<SessionTabId>(() =>
  props.group === 'right' ? (state.value.split?.rightActive ?? state.value.active) : props.group === 'left' ? CONVERSATION : state.value.active,
)
const allIds = computed<SessionTabId[]>(() => [
  ...(showConversation.value ? [CONVERSATION] : []),
  ...shownTabs.value.map((tab) => tab.id),
])
const blocked = computed(() => splitBlockedReason(state.value, uiLayout.windowWidth))
const blockedTip = computed(() => (blocked.value === 'narrow' ? t('run.splitView.blockedNarrow') : t('run.splitView.blockedNoFile')))
const changedCount = useChangedFilesCount()

function tabName(tab: SessionTab): string {
  if (tab.kind === 'file') return tabLabel(tab.path).name
  return changedCount.value === null ? t('run.changedFiles.tabPlain') : t('run.changedFiles.tab', { n: changedCount.value })
}
function tabTip(tab: SessionTab): string {
  return tab.kind === 'file' ? tab.path : t('run.changedFiles.tabTip')
}
function tabIcon(tab: SessionTab): Component {
  return tab.kind === 'file' ? ICONS[fileTabIcon(tab.path)] : FileDiff
}

function select(id: SessionTabId) {
  if (runId.value) tabsStore.select(runId.value, id)
}
function close(id: SessionTabId) {
  if (runId.value) tabsStore.close(runId.value, id)
}

// ── Overflow: which tabs are out of view ───────────────────────────────────
const scroller = ref<HTMLElement | null>(null)
const hidden = ref<SessionTab[]>([])

function measure() {
  const el = scroller.value
  if (!el) return
  const nodes = [...el.querySelectorAll<HTMLElement>('[data-tab-id]')]
  const boxes = nodes.map((n) => ({ left: n.offsetLeft, width: n.offsetWidth }))
  const byId = new Map(shownTabs.value.map((tab) => [tab.id as string, tab]))
  hidden.value = hiddenTabIndexes(boxes, el.scrollLeft, el.clientWidth)
    .map((i) => byId.get(nodes[i].dataset.tabId ?? ''))
    .filter((tab): tab is SessionTab => !!tab)
}

function tabEl(id: SessionTabId): HTMLElement | null {
  return scroller.value?.querySelector<HTMLElement>(`[data-tab-id="${CSS.escape(id)}"]`) ?? null
}

function reveal(id: SessionTabId) {
  tabEl(id)?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
}

function pickHidden(id: SessionTabId) {
  select(id)
  nextTick(() => reveal(id))
}

let observer: ResizeObserver | null = null
onMounted(() => {
  if (typeof ResizeObserver !== 'undefined' && scroller.value) {
    observer = new ResizeObserver(measure)
    observer.observe(scroller.value)
  }
  nextTick(() => {
    reveal(activeId.value)
    measure()
  })
})
onBeforeUnmount(() => observer?.disconnect())

// A tab opened from elsewhere (a link, the file tree) comes into view.
watch(() => [runId.value, activeId.value] as const, () => nextTick(() => {
  reveal(activeId.value)
  measure()
}))
watch([shownTabs, changedCount], () => nextTick(measure))

// ── Drag to reorder ────────────────────────────────────────────────────────
// One sortable per group, so a pinned tab only lands among pinned ones and the
// rest among the rest; each bar has its own scroller, so a split drags within
// its own bar. Indexes are within the group; the store wants the shown order.
const pinnedCount = computed(() => shownTabs.value.filter((tab) => tab.pinned).length)
const pinnedDrag = useDragSort({
  container: scroller,
  itemSelector: '[data-drag-group="pinned"]',
  onDrop: (from, to) => moveTo(from, to),
})
const restDrag = useDragSort({
  container: scroller,
  itemSelector: '[data-drag-group="rest"]',
  onDrop: (from, to) => moveTo(pinnedCount.value + from, pinnedCount.value + to),
})

function moveTo(from: number, to: number) {
  const tab = shownTabs.value[from]
  if (tab && runId.value) tabsStore.move(runId.value, tab.id, to)
}
function dragOf(tab: SessionTab) {
  return tab.pinned ? pinnedDrag : restDrag
}
function groupIndex(tab: SessionTab, i: number): number {
  return tab.pinned ? i : i - pinnedCount.value
}
function startDrag(tab: SessionTab, i: number, e: PointerEvent) {
  dragOf(tab).start(groupIndex(tab, i), e)
}
function isDragged(tab: SessionTab, i: number): boolean {
  return dragOf(tab).dragFrom.value === groupIndex(tab, i)
}
function tabStyle(tab: SessionTab, i: number) {
  return dragOf(tab).itemStyle(groupIndex(tab, i))
}
/** The click that ends a drag drops the tab; it does not select it. */
function onTabClick(id: SessionTabId) {
  if (pinnedDrag.clickWasDrag() || restDrag.clickWasDrag()) return
  select(id)
}

// ── Keyboard: ← → move between tabs, Enter selects, Delete closes ──────────
const focusedId = ref<SessionTabId | null>(null)
const tabStop = computed(() => (focusedId.value && allIds.value.includes(focusedId.value) ? focusedId.value : activeId.value))

function onKeydown(e: KeyboardEvent, id: SessionTabId) {
  const ids = allIds.value
  const i = ids.indexOf(id)
  if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
    e.preventDefault()
    const next = ids[(i + (e.key === 'ArrowRight' ? 1 : -1) + ids.length) % ids.length]
    focusedId.value = next
    nextTick(() => tabEl(next)?.focus())
  } else if (e.key === 'Enter' || e.key === ' ') {
    e.preventDefault()
    select(id)
  } else if ((e.key === 'Delete' || e.key === 'Backspace') && id !== CONVERSATION) {
    e.preventDefault()
    const left = ids[i - 1] ?? ids[i + 1] ?? CONVERSATION
    close(id)
    focusedId.value = left
    nextTick(() => tabEl(left)?.focus())
  }
}

// ── Right-click menu on a file tab ─────────────────────────────────────────
const menu = ref<{ tab: SessionTab; x: number; y: number } | null>(null)

function openMenu(e: MouseEvent, tab: SessionTab) {
  menu.value = { tab, x: e.clientX, y: e.clientY }
}

function target(tab: FileTab) {
  return { path: tab.path, name: tabLabel(tab.path).name, isDir: false }
}

function splitHere(id?: SessionTabId) {
  if (runId.value) tabsStore.startSplit(runId.value, id)
}

// The tab's viewer opens its picker (useFileCompare), so show the tab first.
function compareTab(tab: FileTab) {
  const root = project.value?.path
  if (!root) return
  select(tab.id)
  requestCompare(root, tab.path)
}
</script>

<template>
  <div class="flex h-9 shrink-0 min-w-0 items-stretch border-b border-border bg-background">
    <div
      ref="scroller"
      role="tablist"
      :aria-label="t('run.sessionTabs.tablist')"
      class="session-tabs relative flex min-w-0 flex-1 items-stretch overflow-x-auto"
      @scroll.passive="measure"
    >
      <div
        v-if="showConversation"
        role="tab"
        data-tab-id="conversation"
        :aria-selected="activeId === CONVERSATION"
        :tabindex="tabStop === CONVERSATION ? 0 : -1"
        :title="t('run.sessionTabs.conversationTip')"
        class="session-tab pr-3"
        :class="{ 'is-active': activeId === CONVERSATION }"
        @click="select(CONVERSATION)"
        @focus="focusedId = CONVERSATION"
        @keydown="onKeydown($event, CONVERSATION)"
      >
        <MessageSquare class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" aria-hidden="true" />
        <span class="truncate">{{ t('run.sessionTabs.conversation') }}</span>
      </div>

      <div
        v-for="(tab, i) in shownTabs"
        :key="tab.id"
        role="tab"
        :data-tab-id="tab.id"
        :data-drag-group="tab.pinned ? 'pinned' : 'rest'"
        :aria-selected="activeId === tab.id"
        :tabindex="tabStop === tab.id ? 0 : -1"
        :title="tabTip(tab)"
        class="session-tab pr-1.5"
        :class="{ 'is-active': activeId === tab.id, 'is-dragging': isDragged(tab, i) }"
        :style="tabStyle(tab, i)"
        @pointerdown="startDrag(tab, i, $event)"
        @click="onTabClick(tab.id)"
        @focus="focusedId = tab.id"
        @keydown="onKeydown($event, tab.id)"
        @contextmenu.prevent="openMenu($event, tab)"
      >
        <component :is="tabIcon(tab)" class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" aria-hidden="true" />
        <span class="truncate">{{ tabName(tab) }}</span>
        <button
          v-if="tab.pinned"
          type="button"
          tabindex="-1"
          class="session-tab-btn is-pin"
          :title="t('run.sessionTabs.unpinTab')"
          :aria-label="t('run.sessionTabs.unpinTab')"
          @pointerdown.stop
          @click.stop="tabsStore.togglePin(runId, tab.id)"
        >
          <Pin class="h-3.5 w-3.5" :stroke-width="1.75" />
        </button>
        <button
          type="button"
          tabindex="-1"
          class="session-tab-btn"
          :title="t('run.sessionTabs.closeTab')"
          :aria-label="t('run.sessionTabs.closeTab')"
          @pointerdown.stop
          @click.stop="close(tab.id)"
        >
          <X class="h-3.5 w-3.5" :stroke-width="1.75" />
        </button>
      </div>
    </div>

    <!-- Tabs out of view: "+N ▾" lists them. -->
    <DropdownMenu v-if="hidden.length" align="right" class="shrink-0 items-center">
      <template #trigger>
        <button
          type="button"
          class="mx-1.5 inline-flex h-7 items-center gap-0.5 rounded-md border border-input bg-card pl-2 pr-1.5 text-[13px] text-foreground hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          :title="t('run.sessionTabs.hiddenTip', { n: hidden.length })"
          :aria-label="t('run.sessionTabs.hiddenTip', { n: hidden.length })"
        >
          +{{ hidden.length }}
          <ChevronDown class="h-3.5 w-3.5" :stroke-width="1.75" aria-hidden="true" />
        </button>
      </template>
      <div class="px-2.5 pb-0.5 pt-1 text-2xs font-medium uppercase tracking-wider text-muted-foreground/70">
        {{ t('run.sessionTabs.hiddenTitle', { n: hidden.length }) }}
      </div>
      <DropdownItem v-for="tab in hidden" :key="tab.id" class="max-w-72" @click="pickHidden(tab.id)">
        <component :is="tabIcon(tab)" class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
        <span class="min-w-0 flex-1 truncate">{{ tabName(tab) }}</span>
        <span v-if="tab.kind === 'file' && tabLabel(tab.path).dir" class="max-w-28 truncate font-mono text-xs text-muted-foreground">{{ tabLabel(tab.path).dir }}</span>
      </DropdownItem>
    </DropdownMenu>

    <!-- Conversation tools (UR-07): not in the right bar, which is only file tabs. -->
    <SessionTabTools v-if="group !== 'right'" />

    <!-- Split (V3-27): enabled with a file tab open and a window of 1100px or more. -->
    <button
      v-if="group === 'all'"
      type="button"
      :disabled="!!blocked"
      class="split-btn"
      :title="blocked ? blockedTip : t('run.splitView.splitTip')"
      :aria-label="blocked ? blockedTip : t('run.splitView.splitTip')"
      @click="splitHere()"
    >
      <Columns2 class="h-4 w-4 shrink-0" :stroke-width="1.75" aria-hidden="true" />
    </button>
    <button
      v-else-if="group === 'right'"
      type="button"
      class="split-btn"
      :title="t('run.splitView.unsplitTip')"
      :aria-label="t('run.splitView.unsplitTip')"
      @click="tabsStore.stopSplit(runId)"
    >
      <PanelRightClose class="h-4 w-4 shrink-0" :stroke-width="1.75" aria-hidden="true" />
    </button>

    <ContextMenu :open="!!menu" :x="menu?.x ?? 0" :y="menu?.y ?? 0" @close="menu = null">
      <template v-if="menu">
        <DropdownItem @click="tabsStore.togglePin(runId, menu.tab.id)">
          <component :is="menu.tab.pinned ? PinOff : Pin" class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
          {{ menu.tab.pinned ? t('run.sessionTabs.unpin') : t('run.sessionTabs.pin') }}
        </DropdownItem>
        <DropdownItem @click="close(menu.tab.id)">
          <X class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
          {{ t('run.sessionTabs.close') }}
        </DropdownItem>
        <DropdownItem @click="tabsStore.closeOthers(runId, menu.tab.id)">
          <span class="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          {{ t('run.sessionTabs.closeOthers') }}
        </DropdownItem>
        <DropdownItem
          v-if="group === 'all'"
          :disabled="!!blocked"
          :title="blocked === 'narrow' ? t('run.splitView.blockedNarrow') : undefined"
          @click="splitHere(menu.tab.id)"
        >
          <PanelRight class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
          {{ t('run.splitView.openRight') }}
        </DropdownItem>
        <template v-if="menu.tab.kind === 'file' && project">
          <DropdownSeparator />
          <FileTabMenuItems :project-path="project.path" :target="target(menu.tab)" :can-compare-image="true" @compare="compareTab(menu.tab)" />
        </template>
      </template>
    </ContextMenu>
  </div>
</template>

<style scoped>
.session-tabs {
  scrollbar-width: thin;
}
.session-tab {
  position: relative;
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  gap: 6px;
  min-width: 0;
  max-width: 200px;
  padding-left: 12px;
  border-right: 1px solid hsl(var(--border));
  color: hsl(var(--muted-foreground));
  font-size: 13px;
  white-space: nowrap;
  cursor: pointer;
  /* WebKit (the Tauri webview) still needs the prefix; else a drag selects the label. */
  -webkit-user-select: none;
  user-select: none;
}
.session-tab:hover {
  background: hsl(var(--accent));
  color: hsl(var(--foreground));
}
.session-tab:focus-visible {
  outline: 2px solid hsl(var(--ring));
  outline-offset: -2px;
}
.session-tab.is-active {
  background: hsl(var(--card));
  color: hsl(var(--foreground));
  font-weight: 500;
  margin-bottom: -1px;
  border-bottom: 1px solid hsl(var(--card));
  box-shadow: inset 0 2px 0 hsl(var(--primary));
}
.session-tab.is-dragging {
  background: hsl(var(--card));
  color: hsl(var(--foreground));
  box-shadow: 0 4px 12px hsl(var(--foreground) / 0.18);
  cursor: grabbing;
}
.session-tab-btn {
  display: grid;
  flex-shrink: 0;
  place-items: center;
  width: 28px;
  height: 28px;
  margin: 0 -4px;
  border-radius: 6px;
  color: hsl(var(--muted-foreground));
  cursor: pointer;
}
.split-btn {
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  margin: auto 6px auto 2px;
  border: 1px solid hsl(var(--input));
  border-radius: 6px;
  background: hsl(var(--card));
  color: hsl(var(--foreground));
  cursor: pointer;
}
.split-btn:hover:not(:disabled) {
  background: hsl(var(--accent));
}
.split-btn:focus-visible {
  outline: 2px solid hsl(var(--ring));
  outline-offset: 1px;
}
.split-btn:disabled {
  color: hsl(var(--muted-foreground));
  cursor: not-allowed;
}
.session-tab-btn.is-pin {
  color: hsl(var(--foreground));
}
/* foreground/10, not muted: on the selected tab (card) muted barely shows in dark (RA D8). */
.session-tab-btn:hover {
  background: hsl(var(--foreground) / 0.1);
  color: hsl(var(--foreground));
}
</style>
