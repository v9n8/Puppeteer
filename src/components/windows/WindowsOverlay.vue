<script setup lang="ts">
// "Open windows": a layer over the window in front that lists the private windows
// (not the main one) as cards, to switch to, minimize, move back or close them.
// Mounted by App.vue (main window) and AppPopoutHost.vue (every other window), so
// it opens on whichever window has the focus (lib/openWindowsActions). The list
// comes from stores/openWindows; the rules (filter, groups, keys) from
// lib/windowsOverlay. A window with unsaved changes still asks when closed here.
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { Minus, Search, SquareArrowDownLeft, SquareArrowOutUpRight, TriangleAlert, X } from 'lucide-vue-next'
import { Button, ContextMenu, DropdownItem, DropdownSeparator } from '@/components/ui'
import WindowCard, { type MarkClick } from '@/components/windows/WindowCard.vue'
import WindowsDropStrip from '@/components/windows/WindowsDropStrip.vue'
import WindowsSelectionBar from '@/components/windows/WindowsSelectionBar.vue'
import { useWindowsDrag } from '@/components/windows/useWindowsDrag'
import { useWindowsMarquee } from '@/components/windows/useWindowsMarquee'
import { useToast } from '@/composables/useToast'
import { IS_MAC, effectiveKeymap, hasNativeMenu } from '@/lib/appMenu'
import { isRemoteMode } from '@/lib/ipc/remote'
import { OPEN_WINDOWS_ACTION, formatAccelerator, matchesAccelerator } from '@/lib/keymap'
import { isTopModal, pushModal, removeModal } from '@/lib/modalStack'
import { closeWindow, minimizeWindow, raiseWindow, requestMergeBack } from '@/lib/openWindowsActions'
import { loadRecentWindows } from '@/lib/windowRegistry'
import { thumbFor } from '@/lib/windowThumbs'
import {
  GROUP_STORAGE_KEY, WINDOWS_OVERLAY_OPEN, WINDOW_MERGE_REQUEST,
  isCloseCardKey, runMergeBack, cycleSelection, filterCards, flattenGroups, groupCards, initialSelection,
  moveSelection, parseGroupBy, selectionAfterClose, type GroupBy, type MoveDirection, type WindowTargeted,
} from '@/lib/windowsOverlay'
import { markAll, mergePlan, pruneMarks, rangeMark, toggleMark } from '@/lib/windowsSelection'
import { useOpenWindowsStore } from '@/stores/openWindows'

const props = withDefaults(defineProps<{
  /** Height of the chrome the layer leaves uncovered (the main window's title bar keeps its button in sight). */
  topOffset?: number
}>(), { topOffset: 0 })

const { t } = useI18n()
const { toast } = useToast()
const store = useOpenWindowsStore()

const query = ref('')
const selected = ref<string | null>(null)
const groupBy = ref<GroupBy>('type')
const menu = ref<{ x: number; y: number; label: string } | null>(null)
/** The close question on screen: about every window, or the marked ones. */
const confirmClose = ref<'all' | 'marked' | null>(null)
/** Cards marked for the actions on several windows; the last one marked anchors a ⇧-click range. */
const marks = ref<Set<string>>(new Set())
let markAnchor: string | null = null
const root = ref<HTMLElement | null>(null)
const scroller = ref<HTMLElement | null>(null)
const filterInput = ref<HTMLInputElement | null>(null)
let returnFocusTo: HTMLElement | null = null
// The layer takes a place in the dialog stack (lib/modalStack) like a Modal: a dialog
// opened after it sits above it and takes the keys, one opened before it stays below.
const layerId = Symbol('windows-overlay')
const zIndex = ref(50)

try {
  groupBy.value = parseGroupBy(localStorage.getItem(GROUP_STORAGE_KEY))
} catch {
  /* blocked storage: the default */
}
watch(groupBy, (value) => {
  try {
    localStorage.setItem(GROUP_STORAGE_KEY, value)
  } catch {
    /* blocked storage: not remembered */
  }
})

const total = computed(() => store.cards.length)
const groups = computed(() => groupCards(filterCards(store.cards, query.value), groupBy.value))
const flat = computed(() => flattenGroups(groups.value))
const menuCard = computed(() => (menu.value ? store.cards.find((c) => c.label === menu.value!.label) : undefined))

const keysClose = computed(() => formatAccelerator('CmdOrCtrl+W', IS_MAC))
const closeTip = computed(() => t('windows.closeWindowTip', { keys: keysClose.value }))
const keysMarkAll = computed(() => formatAccelerator('CmdOrCtrl+A', IS_MAC))
const canMergeAny = computed(() => store.cards.some((c) => c.canMerge))
const mergeableMarks = computed(() => store.cards.filter((c) => c.canMerge && marks.value.has(c.label)).length)
const closeLabels = computed(() =>
  confirmClose.value === 'marked' ? [...marks.value] : store.cards.map((c) => c.label),
)
const confirmTitle = computed(() =>
  confirmClose.value === 'marked'
    ? t('windows.confirm.titleSome', { count: closeLabels.value.length }, closeLabels.value.length)
    : t('windows.confirm.title'),
)

function groupName(g: { key: string; isNone: boolean }): string {
  if (g.isNone) return t('windows.group.none')
  return groupBy.value === 'type' ? t(`windows.group.${g.key}`) : g.key
}

// ── Opening and closing the layer ──────────────────────────────────────────

watch(
  () => store.open,
  async (open) => {
    if (open) {
      // The layer never renders in remote mode: no place in the stack either.
      if (!isRemoteMode()) zIndex.value = pushModal(layerId)
      returnFocusTo = document.activeElement instanceof HTMLElement ? document.activeElement : null
      query.value = ''
      menu.value = null
      confirmClose.value = null
      clearMarks()
      await store.refresh()
      // Not awaited: the cards are usable (Enter included) while the pictures come in.
      void store.refreshThumbs()
      selected.value = initialSelection(store.cards, store.here, loadRecentWindows())
      await nextTick()
      // A dialog opened during the wait (the palette) keeps its focus.
      if (isTopModal(layerId)) filterInput.value?.focus()
    } else {
      removeModal(layerId)
      returnFocusTo?.focus?.()
      returnFocusTo = null
    }
  },
)

watch(() => store.cycleTick, () => {
  if (store.open) selected.value = cycleSelection(flat.value, selected.value)
})

// A card gone (its window closed) or filtered out: select a card that is still there.
// The same for the marks: a card no longer on screen is not acted on.
watch(flat, (cards) => {
  if (!store.open) return
  if (!cards.some((c) => c.label === selected.value)) selected.value = cards[0]?.label ?? null
  const kept = pruneMarks(marks.value, cards)
  if (kept.size !== marks.value.size) marks.value = kept
})
watch(query, () => {
  selected.value = flat.value[0]?.label ?? null
})
watch(selected, async (label) => {
  if (!label) return
  await nextTick()
  root.value?.querySelector(`[data-window-label="${CSS.escape(label)}"]`)?.scrollIntoView({ block: 'nearest' })
})

// Time for closed windows to leave the list before it is read again.
const CLOSE_SETTLE_MS = 300

function close() {
  store.open = false
}

// ── What a card does ───────────────────────────────────────────────────────

async function activate(label: string | null) {
  if (!label) return
  if (!(await raiseWindow(label))) {
    toast.error(t('windows.gone'))
    void store.refresh()
    return
  }
  close()
}

async function closeCard(label: string) {
  selected.value = selectionAfterClose(flat.value, label)
  // The card goes when the window is really gone (windows:changed), not now: a window
  // with unsaved changes asks first and may stay.
  await closeWindow(label).catch((e) => {
    console.error('[WindowsOverlay] cannot close', label, e)
    toast.error(t('windows.closeFailed'))
  })
  void store.refresh()
}

async function minimizeCard(label: string) {
  await minimizeWindow(label).catch((e) => {
    console.error('[WindowsOverlay] cannot minimize', label, e)
    toast.error(t('windows.minimizeFailed'))
  })
  void store.refresh()
}

async function mergeCard(label: string) {
  await requestMergeBack(label)
  close()
}

/** After the close question: every window, or the marked ones (the layer stays for the rest then). */
async function closeWindows() {
  const labels = closeLabels.value
  const here = labels.includes(store.here) ? store.here : null
  if (confirmClose.value === 'all' || here) close()
  confirmClose.value = null
  clearMarks()
  // Closing this window too: the main one comes to the front first.
  if (here) await raiseWindow('main')
  await Promise.all(
    labels.filter((l) => l !== here).map((l) => closeWindow(l).catch((e) => console.error('[WindowsOverlay] cannot close', l, e))),
  )
  if (here) await closeWindow(here).catch((e) => console.error('[WindowsOverlay] cannot close', here, e))
  // A window with unsaved changes may stay (the user kept it): count what is really gone.
  await new Promise((r) => setTimeout(r, CLOSE_SETTLE_MS))
  await store.refresh()
  const closed = labels.length - store.cards.filter((c) => labels.includes(c.label)).length
  if (closed > 0) toast.success(t('windows.closed', { count: closed }, closed))
}

// A window moves back on its own (the main window opens what it showed), which can take
// longer than a close: wait until they are gone, up to this long.
const MERGE_SETTLE_MS = 2000
const MERGE_POLL_MS = 150

/** "Move back to main window" for each of `labels` that has it, no question asked; a toast says how many went. */
async function mergeWindows(labels: Iterable<string>) {
  const { merge, skip } = mergePlan(store.cards, labels, store.here)
  close()
  for (const l of merge) {
    // One after the other, so the window running this overlay goes last.
    await requestMergeBack(l).catch((e) => console.error('[WindowsOverlay] cannot move back', l, e))
  }
  // A window with unsaved changes may stay (the user kept it): count what is really gone.
  const stillOpen = () => store.cards.filter((c) => merge.includes(c.label)).length
  for (let waited = 0; waited < MERGE_SETTLE_MS; waited += MERGE_POLL_MS) {
    await new Promise((r) => setTimeout(r, MERGE_POLL_MS))
    await store.refresh()
    if (stillOpen() === 0) break
  }
  const merged = merge.length - stillOpen()
  if (merged === 0 && skip.length === 0) return
  const key = skip.length > 0 ? 'windows.mergedSkipped' : 'windows.merged'
  toast.success(t(key, { count: merged, skipped: skip.length }, merged))
}

function mergeAllWindows() {
  // Every window, not only the ones the filter shows (as "Close all").
  void mergeWindows(store.cards.map((c) => c.label))
}

// ── Marking several cards ──────────────────────────────────────────────────

function clearMarks() {
  marks.value = new Set()
  markAnchor = null
}

function markCard(label: string, click: MarkClick) {
  marks.value = click.shift
    ? rangeMark(marks.value, flat.value, markAnchor ?? selected.value, label)
    : toggleMark(marks.value, label)
  markAnchor = label
  selected.value = label
}

async function minimizeMarked() {
  const failed = await Promise.all([...marks.value].map((l) => minimizeWindow(l).then(() => false, (e) => {
    console.error('[WindowsOverlay] cannot minimize', l, e)
    return true
  })))
  if (failed.some(Boolean)) toast.error(t('windows.minimizeFailed'))
  void store.refresh()
}

// Cards dragged onto the drop strip move back to the main window.
const { drag, dragged, lead: dragLead, onCardPointerDown, consumeClick } = useWindowsDrag({
  cards: () => flat.value,
  marks: () => marks.value,
  onDrop: (labels) => void mergeWindows(labels),
})

// A box dragged from empty space of the grid marks the cards it touches.
const { box: selectBox, onPointerDown: onGridPointerDown } = useWindowsMarquee({
  scroller,
  marks: () => marks.value,
  setMarks: (next) => { marks.value = next },
})

function openMenu(e: MouseEvent, label: string) {
  menu.value = { x: e.clientX, y: e.clientY, label }
}

// ── Keyboard ───────────────────────────────────────────────────────────────

/** Cards per row of the first block, to turn ↑ / ↓ into a jump of one row. */
function columns(): number {
  const cards = [...(root.value?.querySelectorAll<HTMLElement>('[data-window-label]') ?? [])]
  if (cards.length === 0) return 1
  const top = cards[0].offsetTop
  return Math.max(1, cards.filter((c) => c.offsetTop === top).length)
}

const ARROWS: Record<string, MoveDirection> = {
  ArrowLeft: 'left', ArrowRight: 'right', ArrowUp: 'up', ArrowDown: 'down',
}

function isOpenShortcut(e: KeyboardEvent): boolean {
  // The main window's menu bar fires it itself (lib/openWindowsActions through App.vue).
  if (hasNativeMenu()) return false
  const accel = effectiveKeymap().get(OPEN_WINDOWS_ACTION)
  return !!accel && matchesAccelerator(e, accel, IS_MAC)
}

function onKeydown(e: KeyboardEvent) {
  if (e.defaultPrevented || e.isComposing || isRemoteMode()) return
  if (isOpenShortcut(e)) {
    e.preventDefault()
    store.openOrCycle()
    return
  }
  if (!store.open) return
  // A dialog over the layer takes every key but the open shortcut above. One that takes no
  // focus ("Discard changes", the quit question) leaves it in the layer: drop it there, so
  // the key neither types into the filter nor presses a button under the dialog.
  if (!isTopModal(layerId)) {
    if (e.target instanceof HTMLElement && root.value?.contains(e.target)) e.target.blur()
    return
  }
  // Esc while dragging a card or a box cancels it (useWindowsDrag / useWindowsMarquee), nothing else.
  if (drag.value || selectBox.value) return
  // A dialog or menu on top takes its own keys (the context menu closes on Esc itself).
  if (menu.value) return
  if (confirmClose.value) {
    if (e.key === 'Escape') {
      e.preventDefault()
      confirmClose.value = null
    }
    return
  }
  if (e.key === 'Escape') {
    e.preventDefault()
    // Closing takes the layer off the stack in a microtask that runs before a dialog
    // under it hears this Esc (window bubble listener): that dialog would close too.
    e.stopPropagation()
    // The marks go first, so a second Esc is needed to clear the filter or leave.
    if (marks.value.size) clearMarks()
    else if (query.value) query.value = ''
    else close()
    return
  }
  // Space / ⌘A are text keys while the filter has text in it; Space on a button presses it.
  const typingInFilter = e.target === filterInput.value && query.value !== ''
  if (e.key === ' ' && !typingInFilter && !(e.target instanceof HTMLButtonElement) && selected.value) {
    e.preventDefault()
    markCard(selected.value, { shift: false, meta: false })
    return
  }
  if ((IS_MAC ? e.metaKey : e.ctrlKey) && e.code === 'KeyA' && !e.shiftKey && !e.altKey && !typingInFilter) {
    e.preventDefault()
    marks.value = markAll(flat.value)
    return
  }
  const direction = ARROWS[e.key]
  if (direction) {
    e.preventDefault()
    selected.value = moveSelection(flat.value, selected.value, direction, columns())
    return
  }
  if (e.key === 'Enter') {
    e.preventDefault()
    void activate(selected.value)
    return
  }
  if (isCloseCardKey(e, IS_MAC, typingInFilter) && selected.value) {
    e.preventDefault()
    void closeCard(selected.value)
    return
  }
  // A letter pressed with the focus on a card: it belongs in the filter.
  if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey && e.target !== filterInput.value) {
    filterInput.value?.focus()
  }
}

// ── Other windows asking this one ──────────────────────────────────────────

const unlisten: UnlistenFn[] = []

onMounted(() => {
  store.start()
  window.addEventListener('keydown', onKeydown, true)
  const onTargeted = (event: string, run: () => void) =>
    listen<WindowTargeted>(event, (e) => {
      if (e.payload?.target && e.payload.target === store.here) run()
    })
      .then((off) => unlisten.push(off))
      .catch(() => {})
  void onTargeted(WINDOWS_OVERLAY_OPEN, () => store.openOrCycle())
  // This window's own "Move back to main window", asked from the overlay of another window.
  void onTargeted(WINDOW_MERGE_REQUEST, () => {
    void runMergeBack().catch((e) => console.error('[WindowsOverlay] cannot move back to the main window', store.here, e))
  })
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown, true)
  unlisten.forEach((off) => off())
  removeModal(layerId)
})
</script>

<template>
  <div
    v-if="store.open && !isRemoteMode()"
    ref="root"
    class="fixed inset-x-0 bottom-0 flex justify-center"
    :style="{ top: `${props.topOffset}px`, zIndex }"
    role="dialog"
    :aria-label="t('windows.title')"
  >
    <div class="absolute inset-0 bg-black/50" @click="close" />

    <div class="relative my-3 flex w-[calc(100%-24px)] max-w-[1120px] flex-col overflow-hidden rounded-xl border border-popover-border bg-popover text-popover-foreground">
      <!-- One row: the filter gives up width (down to 150px) rather than push a button to a second row. -->
      <header class="flex items-center gap-2 border-b border-border px-3 py-2.5">
        <h2 class="whitespace-nowrap text-md font-semibold">{{ t('windows.title') }}</h2>
        <span class="whitespace-nowrap text-md text-muted-foreground tabular-nums">· {{ query ? `${flat.length} / ${total}` : total }}</span>

        <label class="ml-2 flex h-8 w-[240px] min-w-[150px] max-[901px]:w-[150px] items-center gap-1.5 rounded-md border border-input bg-card px-2.5 text-muted-foreground focus-within:ring-2 focus-within:ring-ring">
          <Search class="h-4 w-4 shrink-0" :stroke-width="1.75" aria-hidden="true" />
          <input
            ref="filterInput"
            v-model="query"
            type="text"
            class="min-w-0 flex-1 bg-transparent text-[13px] text-foreground outline-none placeholder:text-muted-foreground"
            :placeholder="t('windows.filter')"
            :aria-label="t('windows.filterLabel')"
          />
        </label>

        <div class="flex h-[30px] shrink-0 gap-0.5 rounded-md border border-border bg-muted p-0.5 text-sm font-normal" role="group">
          <button
            v-for="mode in (['type', 'project'] as const)"
            :key="mode"
            type="button"
            class="h-6 cursor-pointer rounded px-2.5 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            :class="groupBy === mode ? 'bg-card text-foreground shadow-sm ring-1 ring-border' : 'text-muted-foreground hover:text-foreground'"
            :aria-pressed="groupBy === mode"
            @click="groupBy = mode"
          >{{ t(`windows.by.${mode}`) }}</button>
        </div>

        <span class="flex-1" />
        <!-- Under 1100px both take their short label (the full one is in the tooltip), so the header stays one row at 900. -->
        <Button
          variant="outline"
          size="sm"
          class="text-[13px]"
          :disabled="!canMergeAny"
          :title="`${t('windows.mergeAll')} · ${t('windows.mergeAllTip')}`"
          @click="mergeAllWindows"
        >
          <SquareArrowDownLeft class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
          <span class="max-[1100px]:hidden">{{ t('windows.mergeAll') }}</span>
          <span class="min-[1100px]:hidden">{{ t('windows.mergeAllShort') }}</span>
        </Button>
        <Button
          variant="outline"
          size="sm"
          class="text-[13px]"
          :disabled="total === 0"
          :title="`${t('windows.closeAll')} · ${t('windows.closeAllTip')}`"
          @click="confirmClose = 'all'"
        >
          <X class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
          <span class="max-[1100px]:hidden">{{ t('windows.closeAll') }}</span>
          <span class="min-[1100px]:hidden">{{ t('windows.closeAllShort') }}</span>
        </Button>
        <Button variant="ghost" size="icon-sm" class="shrink-0" :title="t('windows.exit')" :aria-label="t('windows.exit')" @click="close">
          <X class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
        </Button>
      </header>

      <div
        v-if="store.thumbsFailed"
        class="flex flex-wrap items-center gap-2 border-b border-border bg-muted px-4 py-2 text-[13px] text-muted-foreground"
        role="status"
      >
        <TriangleAlert class="h-4 w-4 shrink-0" :stroke-width="1.75" aria-hidden="true" />
        <span class="min-w-0 flex-1">{{ t('windows.thumb.failed') }}</span>
        <Button variant="outline" size="sm" class="text-[13px]" @click="store.refreshThumbs()">{{ t('windows.thumb.retry') }}</Button>
      </div>

      <div
        ref="scroller"
        class="relative min-h-[160px] flex-1 scroll-pt-24 overflow-y-auto px-4 pt-3 pb-4"
        :class="drag ? 'cursor-grabbing select-none' : selectBox ? 'select-none' : ''"
        @pointerdown="onGridPointerDown"
      >
        <p v-if="total === 0" class="py-10 text-center text-sm text-muted-foreground">{{ t('windows.empty') }}</p>
        <p v-else-if="flat.length === 0" class="py-10 text-center text-sm text-muted-foreground">
          {{ t('windows.noMatch', { query: query.trim() }) }}
          <Button variant="ghost" size="sm" class="text-[13px]" @click="query = ''">{{ t('windows.clearFilter') }}</Button>
        </p>
        <template v-else>
        <WindowsDropStrip :dragged="dragged.length" :mergeable="dragged.filter((c) => c.canMerge).length" :name="dragLead?.title ?? ''" />
        <section v-for="g in groups" :key="g.key" class="mb-4 last:mb-0">
          <h3 class="mb-2 text-sm font-semibold text-muted-foreground">{{ groupName(g) }} · {{ g.cards.length }}</h3>
          <div
            class="grid grid-cols-[repeat(auto-fill,184px)] gap-3 min-[1366px]:grid-cols-[repeat(auto-fill,200px)]"
            role="listbox"
            :aria-label="groupName(g)"
          >
            <WindowCard
              v-for="card in g.cards"
              :key="card.label"
              :card="card"
              :thumb="thumbFor(card, store.thumbs, store.thumbStatus[card.label])"
              :selected="card.label === selected"
              :close-tip="closeTip"
              :marked="marks.has(card.label)"
              :mark-mode="marks.size > 0"
              :dragging="!!drag?.labels.includes(card.label)"
              @press="onCardPointerDown($event, card.label)"
              @activate="consumeClick() || activate(card.label)"
              @mark="consumeClick() || markCard(card.label, $event)"
              @close="closeCard(card.label)"
              @select="selected = card.label"
              @menu="openMenu($event, card.label)"
            />
          </div>
        </section>
        </template>
        <!-- Under the sticky strip (z-[2]), over the cards. -->
        <div
          v-if="selectBox"
          data-windows-box
          class="pointer-events-none absolute z-[1] rounded-sm border border-primary bg-primary/10"
          :style="{ left: `${selectBox.left}px`, top: `${selectBox.top}px`, width: `${selectBox.right - selectBox.left}px`, height: `${selectBox.bottom - selectBox.top}px` }"
          aria-hidden="true"
        />
      </div>

      <WindowsSelectionBar
        v-if="marks.size > 0"
        :count="marks.size"
        :mergeable="mergeableMarks"
        :boxing="!!selectBox"
        @merge="mergeWindows(marks)"
        @minimize="minimizeMarked"
        @close="confirmClose = 'marked'"
        @clear="clearMarks"
      />
      <!-- A rule between hints. The row is pulled left by one gap under a clipping box, so the
           rule before the first hint of each line (the footer wraps at 900) is cut off. -->
      <footer v-else class="border-t border-border px-4 py-2 text-[13px] text-muted-foreground">
        <div class="overflow-hidden">
          <div class="-ml-[11px] flex flex-wrap items-center gap-y-1 [&>span]:border-l [&>span]:border-border [&>span]:px-2.5">
            <span><kbd>←</kbd> <kbd>→</kbd> <kbd>↑</kbd> <kbd>↓</kbd> {{ t('windows.foot.select') }}</span>
            <span><kbd>Enter</kbd> {{ t('windows.foot.open') }}</span>
            <span><kbd>Space</kbd> {{ t('windows.foot.mark') }}</span>
            <span><kbd>{{ keysMarkAll }}</kbd> {{ t('windows.foot.markAll') }}</span>
            <span>{{ t('windows.foot.box') }}</span>
            <span><kbd>{{ keysClose }}</kbd> / <kbd>Delete</kbd> {{ t('windows.foot.close') }}</span>
            <span>{{ t('windows.foot.type') }}</span>
            <span><kbd>Esc</kbd> {{ t('windows.foot.exit') }}</span>
          </div>
        </div>
      </footer>
    </div>

    <!-- The dragged card, under the pointer. -->
    <div
      v-if="drag && dragLead"
      class="pointer-events-none fixed z-[3] rotate-2 rounded-[10px] shadow-xl shadow-black/30"
      :style="{ left: `${drag.x - drag.dx}px`, top: `${drag.y - drag.dy}px`, width: `${drag.width}px` }"
      aria-hidden="true"
      inert
    >
      <WindowCard :card="dragLead" :thumb="thumbFor(dragLead, store.thumbs, store.thumbStatus[dragLead.label])" :selected="false" :close-tip="closeTip" :marked="false" :mark-mode="false" />
    </div>

    <ContextMenu :open="!!menu" :x="menu?.x ?? 0" :y="menu?.y ?? 0" @close="menu = null">
      <DropdownItem @click="activate(menu!.label)">
        <SquareArrowOutUpRight class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
        <span class="flex-1">{{ t('windows.open') }}</span>
        <span class="text-xs text-muted-foreground">Enter</span>
      </DropdownItem>
      <DropdownItem :disabled="!menuCard || !menuCard.canMerge" @click="mergeCard(menu!.label)">
        <SquareArrowDownLeft class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
        <span class="flex-1">{{ t('windows.mergeBack') }}</span>
      </DropdownItem>
      <DropdownItem @click="minimizeCard(menu!.label)">
        <Minus class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
        <span class="flex-1">{{ t('windows.minimize') }}</span>
      </DropdownItem>
      <DropdownSeparator />
      <DropdownItem variant="destructive" @click="closeCard(menu!.label)">
        <X class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
        <span class="flex-1">{{ t('windows.closeWindow') }}</span>
        <span class="text-xs">{{ keysClose }}</span>
      </DropdownItem>
    </ContextMenu>

    <div v-if="confirmClose" class="absolute inset-0 z-10 flex items-center justify-center bg-black/40" @click.self="confirmClose = null">
      <div class="w-[420px] max-w-[calc(100%-24px)] rounded-xl border border-border bg-background p-5 shadow-2xl" role="alertdialog" :aria-label="confirmTitle">
        <h3 class="text-sm font-semibold">{{ confirmTitle }}</h3>
        <p class="mt-2 text-[13px] text-muted-foreground">
          {{ t(closeLabels.includes(store.here) ? 'windows.confirm.bodyHere' : 'windows.confirm.body', { count: closeLabels.length }, closeLabels.length) }}
        </p>
        <div class="mt-4 flex justify-end gap-2">
          <Button variant="outline" size="sm" class="text-[13px]" @click="confirmClose = null">{{ t('windows.confirm.cancel') }}</Button>
          <Button variant="destructive" size="sm" class="bg-destructive text-[13px] text-white hover:bg-destructive hover:opacity-90" @click="closeWindows">{{ t('windows.confirm.action', { count: closeLabels.length }, closeLabels.length) }}</Button>
        </div>
      </div>
    </div>
  </div>
</template>
