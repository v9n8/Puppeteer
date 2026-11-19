<script setup lang="ts">
// Plan usage as one block on the status bar (DS-1, option A): the Session's
// account with the % of its most used window, a chip for each other account at
// the warning tier or over, "+n" for the rest. Hovering shows UsagePanel with
// every account; clicking pins it until Esc, a click outside it, or another
// click on the button (as the rings did, UR-39). Rules: lib/usageRing.
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { ChevronUp } from 'lucide-vue-next'
import UsageRing from '@/components/UsageRing.vue'
import UsagePanel from '@/components/UsagePanel.vue'
import {
  ringSeverity, summaryRows, topWindow,
  type UsageRingAccount, type UsageSeverity, type UsageThresholds,
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

function part(row: UsageRingAccount) {
  const top = topWindow(row.windows)
  const severity = ringSeverity(row.windows, props.thresholds)
  return {
    key: row.key,
    label: row.label,
    percent: top?.percent ?? null,
    text: SEV_TEXT[severity],
    severity,
    title: top
      ? t('misc.budget.usage.windowOf', { label: row.label, percent: top.percent, window: t(`misc.budget.windows.${top.key}`) })
      : t('misc.budget.ringLabelNoData', { label: row.label }),
  }
}

const summary = computed(() => {
  const s = summaryRows(props.rows, props.thresholds, props.currentKey)
  return { current: s.current ? part(s.current) : null, hot: s.hot.map(part), restCount: s.restCount }
})

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
  bind(false)
  if (closeTimer) clearTimeout(closeTimer)
})
</script>

<template>
  <button
    v-if="summary.current"
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
    <span class="flex min-w-0 items-center gap-1.5" :title="panelVisible ? undefined : summary.current.title">
      <UsageRing :percent="summary.current.percent ?? 0" :severity="summary.current.severity" />
      <span class="max-w-24 truncate max-[901px]:hidden">{{ summary.current.label }}</span>
      <span v-if="summary.current.percent === null" class="font-mono tabular-nums">–</span>
      <span v-else class="font-mono font-semibold tabular-nums" :class="summary.current.text">{{ summary.current.percent }}%</span>
    </span>
    <span
      v-for="h in summary.hot"
      :key="h.key"
      class="flex items-center gap-1 rounded-full border border-border px-1.5 py-px"
      :title="panelVisible ? undefined : h.title"
      data-usage-hot
    >
      <UsageRing :percent="h.percent ?? 0" :severity="h.severity" size="h-3 w-3" />
      <span class="max-w-20 truncate max-[901px]:hidden">{{ h.label }}</span>
      <span class="font-mono font-semibold tabular-nums" :class="h.text">{{ h.percent }}%</span>
    </span>
    <span
      v-if="summary.restCount > 0"
      class="tabular-nums"
      :title="panelVisible ? undefined : t('misc.budget.usage.more', { n: summary.restCount })"
      data-usage-more
    >+{{ summary.restCount }}</span>
    <ChevronUp class="h-3 w-3 shrink-0" :stroke-width="2" />
  </button>

  <!-- Teleported and `fixed`, so the status bar's overflow can't clip it. -->
  <Teleport to="body">
    <div
      v-if="panelVisible && summary.current"
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
