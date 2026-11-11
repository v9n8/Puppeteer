<script lang="ts">
import { ref } from 'vue'

// The pinned card, shared by every ring: pinning one unpins the others, so at
// most one card stays open.
const pinnedKey = ref<string | null>(null)
</script>

<script setup lang="ts">
// One account's plan usage on the session status bar (UX-23): a donut of the 5h
// window (or the most used window when there is no 5h one). Hovering shows a
// card with every window and its reset time; clicking pins the card until Esc,
// a click outside it, or another click on the ring. Colours: lib/usageRing.ts.
import { computed, onBeforeUnmount, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { RefreshCw } from 'lucide-vue-next'
import {
  ringSeverity, ringWindow, resetCountdown, windowSeverity,
  type UsageRingAccount, type UsageSeverity, type UsageThresholds, type UsageWindow,
} from '@/lib/usageRing'

const props = defineProps<{
  account: UsageRingAccount
  thresholds: UsageThresholds
  /** Clock for the reset countdowns, ticked by the parent. */
  now: number
}>()
const emit = defineEmits<{ (e: 'refresh'): void }>()

const { t } = useI18n()

const SEV_TEXT: Record<UsageSeverity, string> = { ok: 'text-info', warn: 'text-warning', over: 'text-destructive' }
const SEV_STROKE: Record<UsageSeverity, string> = { ok: 'stroke-info', warn: 'stroke-warning', over: 'stroke-destructive' }
const SEV_BG: Record<UsageSeverity, string> = { ok: 'bg-info', warn: 'bg-warning', over: 'bg-destructive' }

const shown = computed(() => ringWindow(props.account.windows))
const ringPct = computed(() => Math.min(100, Math.max(0, shown.value?.percent ?? 0)))
const severity = computed(() => ringSeverity(props.account.windows, props.thresholds))
// An account without plan numbers yet (UX-63): an empty ring whose card offers the refresh.
const noData = computed(() => props.account.windows.length === 0)
const ringTitle = computed(() =>
  noData.value
    ? t('misc.budget.ringLabelNoData', { label: props.account.label })
    : t('misc.budget.ringLabel', { label: props.account.label, percent: shown.value?.percent ?? 0 }),
)

const rows = computed(() =>
  props.account.windows.map((w) => ({
    w,
    label: t(`misc.budget.windows.${w.key}`),
    bar: Math.min(100, Math.max(0, w.percent)),
    severity: windowSeverity(w, props.thresholds),
    reset: resetText(w),
  })),
)

function resetText(w: UsageWindow): string {
  if (w.rolledOver) return t('misc.budget.windowReset')
  const c = resetCountdown(w.reset, props.now)
  if (!c) return ''
  if (c.due) return t('misc.budget.resetsSoon')
  if (c.days > 0) return c.hours > 0 ? t('misc.budget.resetsInDaysHours', c) : t('misc.budget.resetsInDays', c)
  if (c.hours > 0) return c.minutes > 0 ? t('misc.budget.resetsInHoursMins', c) : t('misc.budget.resetsInHours', c)
  return t('misc.budget.resetsInMins', c)
}

const updatedText = computed(() => {
  const at = props.account.capturedAt ? Date.parse(props.account.capturedAt) : NaN
  if (!Number.isFinite(at)) return ''
  const totalMinutes = Math.max(1, Math.floor(Math.max(0, props.now - at) / 60_000))
  let ago: string
  if (totalMinutes < 60) {
    ago = t('misc.budget.agoMins', { minutes: totalMinutes })
  } else if (totalMinutes < 1440) {
    const hours = Math.floor(totalMinutes / 60)
    const minutes = totalMinutes % 60
    ago = minutes > 0 ? t('misc.budget.agoHoursMins', { hours, minutes }) : t('misc.budget.agoHours', { hours })
  } else {
    ago = t('misc.budget.agoDays', { days: Math.floor(totalMinutes / 1440) })
  }
  return t('misc.budget.updated', { ago })
})

// "Probe completed without a fresh snapshot" is not a failure worth showing:
// the stored numbers are still the latest ones.
const errorText = computed(() => {
  const e = props.account.refreshError
  if (!e || e.includes('without a fresh usage snapshot')) return ''
  return t('misc.budget.refreshFailedReason', { error: e })
})

// ── open / pin ──────────────────────────────────────────────────────────────
// Long enough to move the pointer across the gap from the ring onto the card.
const HOVER_CLOSE_DELAY = 150
const GAP = 8
const EDGE_MARGIN = 8
const CARD_WIDTH = 288 // w-72

const trigger = ref<HTMLElement | null>(null)
const card = ref<HTMLElement | null>(null)
const hovered = ref(false)
const pos = ref({ left: 0, bottom: 0 })
let closeTimer: ReturnType<typeof setTimeout> | null = null

const pinned = computed(() => pinnedKey.value === props.account.key)
const cardVisible = computed(() => pinned.value || hovered.value)

function place() {
  const r = trigger.value?.getBoundingClientRect()
  if (!r) return
  // Anchored by its bottom edge so it grows upward from the bar whatever its height.
  const left = Math.max(EDGE_MARGIN, Math.min(r.left, window.innerWidth - CARD_WIDTH - EDGE_MARGIN))
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
  pinnedKey.value = pinned.value ? null : props.account.key
}
function close() {
  if (pinned.value) pinnedKey.value = null
  hovered.value = false
}

function onDocPointer(e: Event) {
  const target = e.target
  if (target instanceof Node && (trigger.value?.contains(target) || card.value?.contains(target))) return
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
watch(cardVisible, (v) => {
  if (v) place()
  bind(v)
})
onBeforeUnmount(() => {
  bind(false)
  if (closeTimer) clearTimeout(closeTimer)
  if (pinned.value) pinnedKey.value = null
})
</script>

<template>
  <button
    ref="trigger"
    type="button"
    class="inline-flex h-5 min-w-0 shrink-0 items-center gap-1 rounded px-1.5 text-2xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
    :class="{ 'bg-muted text-foreground': pinned }"
    :title="cardVisible ? undefined : ringTitle"
    :aria-label="ringTitle"
    aria-haspopup="dialog"
    :aria-expanded="pinned"
    @click="togglePin"
    @mouseenter="enter"
    @mouseleave="leave"
  >
    <svg class="h-3.5 w-3.5 shrink-0" viewBox="0 0 36 36" aria-hidden="true">
      <circle class="fill-none stroke-muted" stroke-width="5" cx="18" cy="18" r="15.9155" />
      <circle
        v-if="ringPct > 0"
        class="fill-none transition-[stroke-dasharray] duration-300 motion-reduce:transition-none"
        :class="SEV_STROKE[severity]"
        stroke-width="5"
        stroke-linecap="round"
        cx="18"
        cy="18"
        r="15.9155"
        :style="{ strokeDasharray: `${ringPct} 100` }"
        transform="rotate(-90 18 18)"
      />
    </svg>
    <span class="max-w-16 truncate">{{ account.label }}</span>
    <span v-if="noData" class="font-mono tabular-nums">–</span>
    <span v-else class="font-mono font-semibold tabular-nums" :class="SEV_TEXT[severity]">{{ shown?.percent ?? 0 }}%</span>
  </button>

  <!-- Teleported and `fixed`, so the status bar's overflow can't clip it. -->
  <Teleport to="body">
    <div
      v-if="cardVisible"
      ref="card"
      role="dialog"
      :aria-label="ringTitle"
      class="ui-popover fixed z-[100] flex w-72 flex-col gap-2.5 rounded-md border border-border bg-popover p-3 text-popover-foreground shadow-lg shadow-black/20"
      :style="{ left: `${pos.left}px`, bottom: `${pos.bottom}px` }"
      @mouseenter="enter"
      @mouseleave="leave"
    >
      <div class="flex items-center gap-2">
        <span class="min-w-0 flex-1 truncate text-xs font-semibold">
          {{ account.label }}<span v-if="account.label !== account.engine" class="font-normal text-muted-foreground"> · {{ account.engine }}</span>
        </span>
        <button
          type="button"
          class="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:cursor-default disabled:hover:bg-transparent"
          :disabled="account.refreshing"
          :aria-label="t('misc.budget.refreshUsage', { label: account.label })"
          :title="t('misc.budget.refreshUsageTitle', { label: account.label })"
          @click="emit('refresh')"
        >
          <RefreshCw class="h-3 w-3" :class="{ 'animate-spin': account.refreshing }" :stroke-width="2" />
        </button>
      </div>

      <p v-if="noData" class="text-2xs text-muted-foreground">{{ t('misc.budget.noData') }}</p>
      <div v-for="r in rows" :key="r.w.key" class="flex flex-col gap-1">
        <div class="flex items-baseline gap-2 text-2xs">
          <span class="min-w-0 flex-1 truncate text-muted-foreground">{{ r.label }}</span>
          <span class="shrink-0 font-mono font-semibold tabular-nums" :class="SEV_TEXT[r.severity]">{{ r.w.percent }}%</span>
          <span v-if="r.reset" class="shrink-0 text-muted-foreground">· {{ r.reset }}</span>
        </div>
        <span class="h-1.5 overflow-hidden rounded-full bg-muted">
          <span class="block h-full rounded-full" :class="SEV_BG[r.severity]" :style="{ width: `${r.bar}%` }" />
        </span>
      </div>

      <div v-if="updatedText || errorText" class="flex flex-col gap-0.5 border-t border-border pt-2 text-2xs text-muted-foreground">
        <span v-if="updatedText">{{ updatedText }}</span>
        <span v-if="errorText" class="line-clamp-2 text-destructive" :title="errorText">{{ errorText }}</span>
      </div>
    </div>
  </Teleport>
</template>
