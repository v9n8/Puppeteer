<script setup lang="ts">
// Plan usage as one block on the status bar (DS-1, option A, revised): every
// account with the % of its 5h (Session) window, as many as fit in half the
// bar, "+n" for the rest. Hovering shows UsagePanel with every account;
// clicking pins it until Esc, a click outside it, or another click on the
// button (as the rings did, UR-39). Rules: lib/usageRing.
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { AlertTriangle, ChevronUp } from 'lucide-vue-next'
import UsagePanel from '@/components/UsagePanel.vue'
import {
  barCell, barRows, windowSeverity,
  type UsageRingAccount, type UsageSeverity, type UsageThresholds, type UsageWindow,
} from '@/lib/usageRing'

const props = defineProps<{
  rows: UsageRingAccount[]
  thresholds: UsageThresholds
  /** Clock for the reset countdowns, ticked by the parent. */
  now: number
  currentKey: string | null
  defaultKeys: string[]
}>()
const emit = defineEmits<{
  (e: 'refresh', key: string): void
  (e: 'refresh-all'): void
}>()

const { t } = useI18n()
const router = useRouter()

const SEV_TEXT: Record<UsageSeverity, string> = { ok: 'text-info', warn: 'text-warning', over: 'text-destructive' }

function windowTitle(label: string, w: UsageWindow): string {
  return t('misc.budget.usage.windowOf', { label, percent: w.percent, window: t(`misc.budget.windows.${w.key}`) })
}

const items = computed(() =>
  barRows(props.rows, props.thresholds, props.currentKey).map((row) => {
    const { window, week } = barCell(row.windows, props.thresholds)
    return {
      key: row.key,
      label: row.label,
      current: row.key === props.currentKey,
      percent: window?.percent ?? null,
      text: window ? SEV_TEXT[windowSeverity(window, props.thresholds)] : '',
      week: week ? SEV_TEXT[windowSeverity(week, props.thresholds)] : null,
      title: window
        ? [window, week].filter((w): w is UsageWindow => !!w).map((w) => windowTitle(row.label, w)).join('\n')
        : t('misc.budget.ringLabelNoData', { label: row.label }),
    }
  }),
)

// ── open / pin ──────────────────────────────────────────────────────────────
// Long enough to move the pointer across the gap from the button onto the panel.
const HOVER_CLOSE_DELAY = 150
const GAP = 8
const EDGE_MARGIN = 8
const PANEL_WIDTH = 460

const trigger = ref<HTMLElement | null>(null)
const panel = ref<HTMLElement | null>(null)
const hovered = ref(false)
const pinned = ref(false)
const pos = ref({ left: 0, bottom: 0 })
let closeTimer: ReturnType<typeof setTimeout> | null = null

const panelVisible = computed(() => pinned.value || hovered.value)

// ── fitting half the bar ────────────────────────────────────────────────────
// The block takes at most half the status bar; the accounts that don't fit
// (the last ones of barRows) go into "+n". Measured with every item shown,
// again whenever the bar resizes or an item's text changes.
const BAR_SHARE = 0.5
const ITEM_GAP = 8 // gap-2 between items
const MORE_ROOM = 40 // the "+n" chip and its gap

const list = ref<HTMLElement | null>(null)
const shownCount = ref(Number.POSITIVE_INFINITY)
const hidden = computed(() => items.value.slice(shownCount.value))
let fitting = false
let refit = false

function bar(): HTMLElement | null {
  return trigger.value?.closest('footer') ?? trigger.value?.parentElement ?? null
}

async function fit() {
  // A change while measuring is measured again right after, not dropped.
  if (fitting) {
    refit = true
    return
  }
  fitting = true
  try {
    shownCount.value = Number.POSITIVE_INFINITY
    await nextTick()
    const btn = trigger.value
    const box = list.value
    if (!btn || !box) return
    const widths = Array.from(box.children, (el) => (el as HTMLElement).offsetWidth)
    let room = (bar()?.clientWidth ?? 0) * BAR_SHARE - (btn.offsetWidth - box.offsetWidth)
    const total = widths.reduce((sum, w) => sum + w + ITEM_GAP, -ITEM_GAP)
    if (total > room) room -= MORE_ROOM
    let used = -ITEM_GAP
    const over = widths.findIndex((w) => (used += w + ITEM_GAP) > room)
    // The Session's account always shows, however narrow the bar.
    shownCount.value = over < 0 ? widths.length : Math.max(1, over)
  } finally {
    fitting = false
    if (refit) {
      refit = false
      void fit()
    }
  }
}

// The button only mounts once there are rows, so the observer follows it.
const resizeObserver = new ResizeObserver(() => { void fit() })
watch(trigger, () => {
  resizeObserver.disconnect()
  const el = bar()
  if (el) resizeObserver.observe(el)
  void fit()
}, { flush: 'post' })
watch(() => items.value.map((i) => `${i.key}:${i.label}:${i.percent}:${i.week}`).join('|'), () => { void fit() })

function place() {
  const r = trigger.value?.getBoundingClientRect()
  if (!r) return
  // Anchored by its bottom edge so it grows upward from the bar whatever its height.
  const left = Math.max(EDGE_MARGIN, Math.min(r.left, window.innerWidth - PANEL_WIDTH - EDGE_MARGIN))
  pos.value = { left, bottom: window.innerHeight - r.top + GAP }
}

function enter() {
  if (closeTimer) clearTimeout(closeTimer)
  closeTimer = null
  hovered.value = true
}
function leave() {
  if (closeTimer) clearTimeout(closeTimer)
  closeTimer = setTimeout(() => { hovered.value = false }, HOVER_CLOSE_DELAY)
}
function togglePin() {
  pinned.value = !pinned.value
}
function close() {
  pinned.value = false
  hovered.value = false
}

// The engine's card in Settings is the one place to add, sign in or remove accounts.
function manageAccounts() {
  close()
  router.push({ path: '/settings', query: { section: 'engines' } }).catch(() => {})
}

function onDocPointer(e: Event) {
  const target = e.target
  if (target instanceof Node && (trigger.value?.contains(target) || panel.value?.contains(target))) return
  close()
}
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') close()
}
function onResize() {
  place()
}

function bind(on: boolean) {
  const m = on ? 'addEventListener' : 'removeEventListener'
  document[m]('mousedown', onDocPointer)
  document[m]('keydown', onKey as EventListener)
  window[m]('resize', onResize)
}
watch(panelVisible, (v) => {
  if (v) place()
  bind(v)
})
onBeforeUnmount(() => {
  resizeObserver.disconnect()
  bind(false)
  if (closeTimer) clearTimeout(closeTimer)
})
</script>

<template>
  <button
    v-if="items.length > 0"
    ref="trigger"
    type="button"
    data-usage-summary
    class="inline-flex h-7 min-w-0 shrink-0 items-center gap-1.5 rounded px-1.5 text-2xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
    :class="{ 'bg-accent text-foreground': pinned }"
    :title="panelVisible ? undefined : t('misc.budget.usage.summaryTitle')"
    :aria-label="t('misc.budget.usage.summaryTitle')"
    aria-haspopup="dialog"
    :aria-expanded="pinned"
    @click="togglePin"
    @mouseenter="enter"
    @mouseleave="leave"
  >
    <span ref="list" class="flex min-w-0 items-center gap-2 overflow-hidden">
      <span
        v-for="(item, i) in items"
        v-show="i < shownCount"
        :key="item.key"
        class="flex shrink-0 items-center gap-1"
        :title="panelVisible ? undefined : item.title"
        data-usage-item
      >
        <span v-if="item.current" class="h-1.5 w-1.5 shrink-0 rounded-full bg-primary" />
        <span class="max-w-[72px] truncate" :class="{ 'font-medium text-foreground': item.current }">{{ item.label }}</span>
        <span v-if="item.percent === null" class="font-mono tabular-nums">–</span>
        <span v-else class="font-mono font-semibold tabular-nums" :class="item.text">{{ item.percent }}%</span>
        <AlertTriangle v-if="item.week" class="h-3 w-3 shrink-0" :class="item.week" :stroke-width="2" />
      </span>
    </span>
    <span
      v-if="hidden.length > 0"
      class="shrink-0 rounded-full border border-border px-1.5 py-px tabular-nums"
      :title="panelVisible ? undefined : hidden.map((h) => h.label).join(', ')"
      :aria-label="t('misc.budget.usage.more', { n: hidden.length })"
      data-usage-more
    >+{{ hidden.length }}</span>
    <ChevronUp class="h-3 w-3 shrink-0" :stroke-width="2" />
  </button>

  <!-- Teleported and `fixed`, so the status bar's overflow can't clip it. -->
  <Teleport to="body">
    <div
      v-if="panelVisible && items.length > 0"
      ref="panel"
      class="fixed z-[100]"
      :style="{ left: `${pos.left}px`, bottom: `${pos.bottom}px` }"
      @mouseenter="enter"
      @mouseleave="leave"
    >
      <UsagePanel
        :rows="rows"
        :thresholds="thresholds"
        :now="now"
        :current-key="currentKey"
        :default-keys="defaultKeys"
        @refresh="emit('refresh', $event)"
        @refresh-all="emit('refresh-all')"
        @manage="manageAccounts"
      />
    </div>
  </Teleport>
</template>
