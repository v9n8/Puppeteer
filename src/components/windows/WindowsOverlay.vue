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
import WindowCard from '@/components/windows/WindowCard.vue'
import { useToast } from '@/composables/useToast'
import { IS_MAC, effectiveKeymap, hasNativeMenu } from '@/lib/appMenu'
import { isRemoteMode } from '@/lib/ipc/remote'
import { OPEN_WINDOWS_ACTION, formatAccelerator, matchesAccelerator } from '@/lib/keymap'
import { closeWindow, minimizeWindow, raiseWindow, requestMergeBack } from '@/lib/openWindowsActions'
import { loadRecentWindows } from '@/lib/windowRegistry'
import { thumbFor } from '@/lib/windowThumbs'
import {
  GROUP_STORAGE_KEY, WINDOWS_OVERLAY_OPEN, WINDOW_MERGE_REQUEST,
  isCloseCardKey, runMergeBack, cycleSelection, filterCards, flattenGroups, groupCards, initialSelection,
  moveSelection, parseGroupBy, selectionAfterClose, type GroupBy, type MoveDirection, type WindowTargeted,
} from '@/lib/windowsOverlay'
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
const confirmAll = ref(false)
const root = ref<HTMLElement | null>(null)
const filterInput = ref<HTMLInputElement | null>(null)
let returnFocusTo: HTMLElement | null = null

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
const hereIsPrivate = computed(() => store.cards.some((c) => c.isHere))

function groupName(g: { key: string; isNone: boolean }): string {
  if (g.isNone) return t('windows.group.none')
  return groupBy.value === 'type' ? t(`windows.group.${g.key}`) : g.key
}

// ── Opening and closing the layer ──────────────────────────────────────────

watch(
  () => store.open,
  async (open) => {
    if (open) {
      returnFocusTo = document.activeElement instanceof HTMLElement ? document.activeElement : null
      query.value = ''
      menu.value = null
      confirmAll.value = false
      await store.refresh()
      // Not awaited: the cards are usable (Enter included) while the pictures come in.
      void store.refreshThumbs()
      selected.value = initialSelection(store.cards, store.here, loadRecentWindows())
      await nextTick()
      filterInput.value?.focus()
    } else {
      returnFocusTo?.focus?.()
      returnFocusTo = null
    }
  },
)

watch(() => store.cycleTick, () => {
  if (store.open) selected.value = cycleSelection(flat.value, selected.value)
})

// A card gone (its window closed) or filtered out: select a card that is still there.
watch(flat, (cards) => {
  if (!store.open) return
  if (!cards.some((c) => c.label === selected.value)) selected.value = cards[0]?.label ?? null
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

async function closeAllWindows() {
  const labels = store.cards.map((c) => c.label)
  const here = labels.includes(store.here) ? store.here : null
  confirmAll.value = false
  close()
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
  // A dialog or menu on top takes its own keys (the context menu closes on Esc itself).
  if (menu.value) return
  if (confirmAll.value) {
    if (e.key === 'Escape') {
      e.preventDefault()
      confirmAll.value = false
    }
    return
  }
  if (e.key === 'Escape') {
    e.preventDefault()
    if (query.value) query.value = ''
    else close()
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
  const typingInFilter = e.target === filterInput.value && query.value !== ''
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
})
</script>

<template>
  <div
    v-if="store.open && !isRemoteMode()"
    ref="root"
    class="fixed inset-x-0 bottom-0 z-[60] flex justify-center"
    :style="{ top: `${props.topOffset}px` }"
    role="dialog"
    :aria-label="t('windows.title')"
  >
    <div class="absolute inset-0 bg-black/50" @click="close" />

    <div class="relative my-3 flex w-[calc(100%-24px)] max-w-[1120px] flex-col overflow-hidden rounded-xl border border-border bg-background text-foreground shadow-2xl">
      <header class="flex flex-wrap items-center gap-2 border-b border-border px-4 py-3">
        <h2 class="text-sm font-semibold">{{ t('windows.title') }}</h2>
        <span class="text-sm text-muted-foreground tabular-nums">· {{ query ? `${flat.length} / ${total}` : total }}</span>

        <label class="ml-2 flex h-8 w-[220px] items-center gap-2 rounded-md border border-input bg-background px-2 text-muted-foreground focus-within:ring-2 focus-within:ring-ring">
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

        <div class="flex rounded-md bg-muted p-0.5 text-[13px] font-medium" role="group">
          <button
            v-for="mode in (['type', 'project'] as const)"
            :key="mode"
            type="button"
            class="h-7 cursor-pointer rounded px-2.5 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            :class="groupBy === mode ? 'bg-card text-foreground shadow-sm ring-1 ring-border' : 'text-muted-foreground hover:text-foreground'"
            :aria-pressed="groupBy === mode"
            @click="groupBy = mode"
          >{{ t(`windows.by.${mode}`) }}</button>
        </div>

        <span class="flex-1" />
        <Button variant="outline" size="sm" class="text-[13px]" :disabled="total === 0" :title="t('windows.closeAllTip')" @click="confirmAll = true">
          <X class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
          {{ t('windows.closeAll') }}
        </Button>
        <Button variant="ghost" size="icon-sm" :title="t('windows.exit')" :aria-label="t('windows.exit')" @click="close">
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

      <div class="min-h-[160px] flex-1 overflow-y-auto px-4 py-3">
        <p v-if="total === 0" class="py-10 text-center text-sm text-muted-foreground">{{ t('windows.empty') }}</p>
        <p v-else-if="flat.length === 0" class="py-10 text-center text-sm text-muted-foreground">
          {{ t('windows.noMatch', { query: query.trim() }) }}
          <Button variant="ghost" size="sm" class="text-[13px]" @click="query = ''">{{ t('windows.clearFilter') }}</Button>
        </p>
        <template v-else>
        <section v-for="g in groups" :key="g.key" class="mb-4 last:mb-0">
          <h3 class="mb-2 text-[13px] font-medium text-muted-foreground">{{ groupName(g) }} · {{ g.cards.length }}</h3>
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
              @activate="activate(card.label)"
              @close="closeCard(card.label)"
              @select="selected = card.label"
              @menu="openMenu($event, card.label)"
            />
          </div>
        </section>
        </template>
      </div>

      <footer class="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-border px-4 py-2 text-[13px] text-muted-foreground">
        <span><kbd>←</kbd> <kbd>→</kbd> <kbd>↑</kbd> <kbd>↓</kbd> {{ t('windows.foot.select') }}</span>
        <span><kbd>Enter</kbd> {{ t('windows.foot.open') }}</span>
        <span><kbd>{{ keysClose }}</kbd> / <kbd>Delete</kbd> {{ t('windows.foot.close') }}</span>
        <span>{{ t('windows.foot.type') }}</span>
        <span><kbd>Esc</kbd> {{ t('windows.foot.exit') }}</span>
      </footer>
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

    <div v-if="confirmAll" class="absolute inset-0 z-10 flex items-center justify-center bg-black/40" @click.self="confirmAll = false">
      <div class="w-[420px] max-w-[calc(100%-24px)] rounded-xl border border-border bg-background p-5 shadow-2xl" role="alertdialog" :aria-label="t('windows.confirm.title')">
        <h3 class="text-sm font-semibold">{{ t('windows.confirm.title') }}</h3>
        <p class="mt-2 text-[13px] text-muted-foreground">
          {{ t(hereIsPrivate ? 'windows.confirm.bodyHere' : 'windows.confirm.body', { count: total }, total) }}
        </p>
        <div class="mt-4 flex justify-end gap-2">
          <Button variant="outline" size="sm" class="text-[13px]" @click="confirmAll = false">{{ t('windows.confirm.cancel') }}</Button>
          <Button variant="destructive" size="sm" class="bg-destructive text-[13px] text-white hover:bg-destructive hover:opacity-90" @click="closeAllWindows">{{ t('windows.confirm.action', { count: total }, total) }}</Button>
        </div>
      </div>
    </div>
  </div>
</template>
