<script setup lang="ts">
// The open usage panel (DS-1): one bordered block per engine, a row per
// account with its 5h and Week cells (% + time to reset + bar). An account
// without numbers offers its refresh; the footer links to the accounts in
// Settings. Rules: lib/usageRing; opening and pinning: UsageSummary.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { RefreshCw, Settings, Sparkles, Star, Terminal } from 'lucide-vue-next'
import {
  resetCountdown, usageColumns, windowSeverity,
  type UsageRingAccount, type UsageSeverity, type UsageThresholds, type UsageWindow,
} from '@/lib/usageRing'

const props = defineProps<{
  rows: UsageRingAccount[]
  thresholds: UsageThresholds
  now: number
  /** Row of the account this Session runs on (● "used by this Session"). */
  currentKey: string | null
  /** Rows of the accounts a run gets when it picks none (★). */
  defaultKeys: string[]
}>()
const emit = defineEmits<{
  (e: 'refresh', key: string): void
  (e: 'refresh-all'): void
  (e: 'manage'): void
}>()

const { t } = useI18n()

const SEV_TEXT: Record<UsageSeverity, string> = { ok: 'text-info', warn: 'text-warning', over: 'text-destructive' }
const SEV_BG: Record<UsageSeverity, string> = { ok: 'bg-info', warn: 'bg-warning', over: 'bg-destructive' }
const COLS = 'grid grid-cols-[minmax(0,1fr)_112px_112px] items-center gap-x-4'
const ENGINES = [
  { name: 'Claude', icon: Sparkles },
  { name: 'Codex', icon: Terminal },
]

// The time to reset in short ("5n 14g"); the cell's tooltip names the window.
function resetShort(w: UsageWindow): string {
  if (w.rolledOver) return t('misc.budget.windowReset')
  const c = resetCountdown(w.reset, props.now)
  if (!c) return ''
  if (c.due) return t('misc.budget.resetsSoon')
  if (c.days > 0) return c.hours > 0 ? t('misc.budget.usage.inDaysHours', c) : t('misc.budget.usage.inDays', c)
  if (c.hours > 0) return c.minutes > 0 ? t('misc.budget.usage.inHoursMins', c) : t('misc.budget.usage.inHours', c)
  return t('misc.budget.usage.inMins', c)
}
// "Weekly (Opus): 60% · 3d 9h", one line per window, for a cell's tooltip.
function windowLine(w: UsageWindow): string {
  const reset = resetShort(w)
  return `${t(`misc.budget.windows.${w.key}`)}: ${w.percent}%${reset ? ` · ${reset}` : ''}`
}

function cell(w: UsageWindow | null, all: UsageWindow[]) {
  if (!w) return null
  const severity = windowSeverity(w, props.thresholds)
  return {
    percent: w.percent,
    bar: Math.min(100, Math.max(0, w.percent)),
    reset: resetShort(w),
    text: SEV_TEXT[severity],
    bg: SEV_BG[severity],
    title: all.map(windowLine).join('\n'),
  }
}

// "Probe completed without a fresh snapshot" is not a failure worth showing:
// the stored numbers are still the latest ones.
function errorText(row: UsageRingAccount): string {
  const e = row.refreshError
  if (!e || e.includes('without a fresh usage snapshot')) return ''
  return t('misc.budget.refreshFailedReason', { error: e })
}

const blocks = computed(() =>
  ENGINES.map((eng) => ({
    ...eng,
    rows: props.rows.filter((r) => r.engine === eng.name).map((r) => {
      const c = usageColumns(r.windows)
      return {
        row: r,
        noData: r.windows.length === 0,
        using: r.key === props.currentKey,
        isDefault: props.defaultKeys.includes(r.key),
        fiveHour: cell(c.fiveHour, c.fiveHour ? [c.fiveHour] : []),
        week: cell(c.week, c.weekAll),
        error: errorText(r),
      }
    }),
  })).filter((b) => b.rows.length > 0),
)

// The newest capture across the rows.
const updatedText = computed(() => {
  const at = Math.max(...props.rows.map((r) => (r.capturedAt ? Date.parse(r.capturedAt) : NaN)).filter(Number.isFinite))
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
const refreshingAny = computed(() => props.rows.some((r) => r.refreshing))
</script>

<template>
  <div
    role="dialog"
    :aria-label="t('misc.budget.usage.title')"
    class="ui-popover w-[460px] max-w-[calc(100vw-1rem)] rounded-md border border-border bg-popover p-3 text-xs text-popover-foreground shadow-lg shadow-black/20"
  >
    <div class="mb-2 flex items-center gap-2">
      <span class="flex-1 text-xs font-semibold">{{ t('misc.budget.usage.title') }}</span>
      <button
        type="button"
        class="flex h-7 w-7 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground disabled:cursor-default disabled:hover:bg-transparent"
        :disabled="refreshingAny"
        :title="t('misc.budget.usage.refreshAll')"
        :aria-label="t('misc.budget.usage.refreshAll')"
        @click="emit('refresh-all')"
      >
        <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': refreshingAny }" :stroke-width="2" />
      </button>
    </div>

    <div class="flex flex-col gap-2">
      <section
        v-for="b in blocks"
        :key="b.name"
        class="overflow-hidden rounded-md border border-border bg-card"
        :aria-label="b.name"
      >
        <div class="flex h-8 min-w-0 items-center gap-1.5 border-b border-border bg-muted px-2.5 text-2xs font-semibold text-foreground">
          <component :is="b.icon" class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="2" />
          {{ b.name }}<span class="font-normal text-muted-foreground">· {{ b.rows.length }}</span>
        </div>
        <ul class="divide-y divide-border">
          <li v-for="r in b.rows" :key="r.row.key" :class="COLS" class="min-h-11 px-2.5 py-2" :data-usage-row="r.row.key">
            <div class="flex min-w-0 flex-col">
              <div class="flex min-w-0 items-center gap-1.5">
                <span class="truncate font-medium text-foreground" :title="r.row.label">{{ r.row.label }}</span>
                <span
                  v-if="r.using"
                  class="h-1.5 w-1.5 shrink-0 rounded-full bg-primary"
                  role="img"
                  :title="t('misc.budget.usage.using')"
                  :aria-label="t('misc.budget.usage.using')"
                />
                <span
                  v-if="r.isDefault"
                  class="inline-flex shrink-0"
                  role="img"
                  :title="t('common.default')"
                  :aria-label="t('common.default')"
                >
                  <Star class="h-3 w-3 fill-primary text-primary" :stroke-width="2" />
                </span>
              </div>
              <span v-if="r.error" class="line-clamp-1 text-2xs text-destructive" :title="r.error">{{ r.error }}</span>
            </div>
            <span v-if="r.noData" class="col-span-2 flex items-center justify-between gap-2 text-muted-foreground">
              {{ t('misc.budget.usage.noData') }}
              <button
                type="button"
                class="flex h-7 items-center gap-1 rounded px-1.5 text-muted-foreground hover:bg-accent hover:text-foreground disabled:cursor-default disabled:hover:bg-transparent"
                :disabled="r.row.refreshing"
                :title="t('misc.budget.refreshUsageTitle', { label: r.row.label })"
                :aria-label="t('misc.budget.refreshUsage', { label: r.row.label })"
                @click="emit('refresh', r.row.key)"
              >
                <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': r.row.refreshing }" :stroke-width="2" />
                {{ t('misc.budget.usage.refresh') }}
              </button>
            </span>
            <template v-else>
              <div v-for="(c, i) in [r.fiveHour, r.week]" :key="i" class="min-w-0" :title="c?.title">
                <div v-if="c" class="flex flex-col gap-1">
                  <span class="flex items-baseline gap-1.5">
                    <span class="font-mono font-semibold tabular-nums" :class="c.text">{{ c.percent }}%</span>
                    <span class="truncate text-2xs text-muted-foreground">{{ c.reset }}</span>
                  </span>
                  <span class="h-1.5 overflow-hidden rounded-full bg-muted">
                    <span class="block h-full rounded-full" :class="c.bg" :style="{ width: `${c.bar}%` }" />
                  </span>
                </div>
                <span v-else class="font-mono text-muted-foreground">–</span>
              </div>
            </template>
          </li>
        </ul>
      </section>
    </div>

    <div class="mt-3 flex items-center gap-2 border-t border-border pt-2 text-muted-foreground">
      <span class="min-w-0 flex-1 truncate">{{ updatedText }}</span>
      <button
        type="button"
        class="flex h-7 items-center gap-1.5 rounded px-2 text-foreground hover:bg-accent"
        @click="emit('manage')"
      >
        <Settings class="h-3.5 w-3.5" :stroke-width="1.75" />
        {{ t('run.accountMenu.manage') }}
      </button>
    </div>
  </div>
</template>
