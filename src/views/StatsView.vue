<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { invoke } from '@/lib/tauri'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import {
  Chart as ChartJS,
  Title,
  Tooltip,
  Legend,
  BarController,
  BarElement,
  LineController,
  LineElement,
  PointElement,
  DoughnutController,
  ArcElement,
  CategoryScale,
  LinearScale,
} from 'chart.js'
import { Doughnut } from 'vue-chartjs'
import KeyboardChart from '@/components/stats/KeyboardChart.vue'
import StorageCleanupDialog from '@/components/stats/StorageCleanupDialog.vue'
import {
  Coins, DollarSign, Play, Repeat, RefreshCw, Trash2, Loader2, Info, AlertTriangle, HardDrive, Gauge, Database,
} from 'lucide-vue-next'
import { Button, Input, Card, AppSelect, PageHeader, Banner, InfoTip } from '@/components/ui'
import { useProjectsStore } from '@/stores/projects'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { errorText } from '@/lib/featureGate'
import { isRemoteMode } from '@/lib/ipc/remote'
import { useConfirm } from '@/composables/useConfirm'
import { ESTIMATED, mergeCleanable, type CleanupOutcome, type CleanupPreview } from '@/lib/storageCleanup'
import {
  getUsageStats, backfillUsage, resetUsageStats,
  getStorageStats, previewStorageCleanup, compactDatabase, NO_ACCOUNT,
  type StatsResult, type StatsFilter, type StorageStats, type AccountStat,
} from '@/stores/stats'

// Register controllers explicitly. The mixed bar/line daily chart needs both
// BarController and LineController. Relying on vue-chartjs's import side effects
// (createTypedChart calls Chart.register) breaks in production: those calls are
// marked /* #__PURE__ */, so Rollup tree-shakes away the unused Line component
// and LineController never registers — the daily chart then fails only in builds.
ChartJS.register(
  Title, Tooltip, Legend,
  BarController, BarElement,
  LineController, LineElement, PointElement,
  DoughnutController, ArcElement,
  CategoryScale, LinearScale,
)

const { t, te } = useI18n()
const projectsStore = useProjectsStore()
const accountsStore = useClaudeAccountsStore()

// ── filters ────────────────────────────────────────────────────────────────
const RANGE_OPTIONS = computed(() => [
  { value: '7', label: t('stats.range.last7') },
  { value: '30', label: t('stats.range.last30') },
  { value: '90', label: t('stats.range.last90') },
  { value: 'all', label: t('stats.range.all') },
])
const range = ref('30')
const engine = ref('') // '' = all
const projectId = ref('') // '' = all
const accountId = ref('') // '' = all; NO_ACCOUNT = global ~/.claude

const engineOptions = computed(() => [
  { value: '', label: t('stats.engine.all') },
  { value: 'claude', label: 'Claude' },
  { value: 'codex', label: 'Codex' },
])
const projectOptions = computed(() => [
  { value: '', label: t('stats.project.all') },
  ...projectsStore.projects.map((p) => ({ value: p.id, label: p.name })),
])

// Only meaningful once managed Claude accounts exist; picking one implies Claude.
const hasAccounts = computed(() => accountsStore.accounts.length > 0)
const accountOptions = computed(() => [
  { value: '', label: t('stats.account.all') },
  ...accountsStore.accounts.map((a) => ({ value: a.id, label: a.label })),
  { value: NO_ACCOUNT, label: t('stats.account.none') },
])
function accountName(a: AccountStat): string {
  if (!a.account_id) return t('stats.account.none')
  return a.account_label ?? t('stats.account.deleted')
}

function rangeToDates(r: string): { from: string | null; to: string | null } {
  if (r === 'all') return { from: null, to: null }
  const days = parseInt(r, 10)
  const now = new Date()
  const start = new Date(now)
  start.setDate(now.getDate() - (days - 1))
  const fmt = (d: Date) => d.toISOString().slice(0, 10)
  return { from: fmt(start), to: fmt(now) }
}

const currentFilter = computed<StatsFilter>(() => {
  const { from, to } = rangeToDates(range.value)
  return {
    from,
    to,
    engine: engine.value || null,
    project_id: projectId.value || null,
    account_id: accountId.value || null,
  }
})

// ── data ─────────────────────────────────────────────────────────────────────
const stats = ref<StatsResult | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)

async function load() {
  loading.value = true
  error.value = null
  try {
    stats.value = await getUsageStats(currentFilter.value)
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

watch(currentFilter, load, { deep: true })

onMounted(async () => {
  if (projectsStore.projects.length === 0) await projectsStore.fetchProjects()
  if (accountsStore.accounts.length === 0) await accountsStore.fetch()
  now.value = Date.now()
  clockTimer = setInterval(() => { now.value = Date.now() }, 30_000)
  await Promise.all([load(), loadStorage(), refreshPlanUsage(), refreshPlanUsageCodex()])
  // A cleanup (this button or the daily one) changed what is on disk.
  unlistenCleaned = await listen('storage:cleaned', () => { loadStorage().catch(() => {}) })
  unlistenPlanUsage = await listen<{ provider?: string }>('plan_usage_updated', (e) => {
    if (e.payload?.provider === 'codex') {
      refreshPlanUsageCodex().catch(() => {})
    } else {
      refreshPlanUsage().catch(() => {})
    }
  })
})

onUnmounted(() => {
  if (clockTimer) clearInterval(clockTimer)
  if (unlistenPlanUsage) unlistenPlanUsage()
  if (unlistenCleaned) unlistenCleaned()
})

// ── formatting ────────────────────────────────────────────────────────────────
function fmtNum(n: number): string {
  return n.toLocaleString('en-US')
}
function fmtCompact(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + 'M'
  if (n >= 1_000) return (n / 1_000).toFixed(1) + 'k'
  return String(n)
}
function fmtCost(n: number): string {
  if (n === 0) return '$0'
  if (n < 0.01) return '$' + n.toFixed(4)
  return '$' + n.toFixed(2)
}

const hasData = computed(() => (stats.value?.summary.total_turns ?? 0) > 0)

// ── chart palette ────────────────────────────────────────────────────────────
const INDIGO = '#6366f1'
const AMBER = '#f59e0b'
const PALETTE = ['#6366f1', '#22c55e', '#f59e0b', '#ec4899', '#06b6d4', '#a855f7', '#ef4444', '#14b8a6']

const gridColor = 'rgba(127,127,127,0.12)'
const tickColor = 'rgba(148,148,160,0.9)'

// Daily tokens (bars) + cost (line, right axis).
// Typed `any` because this is a mixed bar/line dataset, which doesn't fit
// vue-chartjs's `ChartData<'bar'>` prop type.
const dailyChartData = computed((): any => {
  const d = stats.value?.daily ?? []
  return {
    labels: d.map((p) => p.date.slice(5)), // MM-DD
    datasets: [
      {
        type: 'bar' as const,
        label: t('stats.charts.tokens'),
        data: d.map((p) => p.tokens),
        backgroundColor: 'rgba(99,102,241,0.55)',
        borderColor: INDIGO,
        borderWidth: 1,
        borderRadius: 3,
        yAxisID: 'y',
        order: 2,
      },
      {
        type: 'line' as const,
        label: t('stats.charts.cost'),
        data: d.map((p) => p.cost),
        borderColor: AMBER,
        backgroundColor: AMBER,
        borderWidth: 2,
        pointRadius: 2,
        tension: 0.3,
        yAxisID: 'y1',
        order: 1,
      },
    ],
  }
})

function describeDay(i: number): string {
  const p = stats.value?.daily[i]
  return p ? t('stats.charts.dailySpoken', { date: p.date, tokens: fmtNum(p.tokens), cost: fmtCost(p.cost) }) : ''
}

const dailyChartOptions = {
  responsive: true,
  maintainAspectRatio: false,
  interaction: { mode: 'index' as const, intersect: false },
  plugins: {
    legend: { labels: { color: tickColor, boxWidth: 12, font: { size: 11 } } },
  },
  scales: {
    x: { grid: { color: gridColor }, ticks: { color: tickColor, font: { size: 10 }, maxRotation: 0, autoSkip: true } },
    y: {
      position: 'left' as const,
      grid: { color: gridColor },
      ticks: { color: tickColor, font: { size: 10 }, callback: (v: number | string) => fmtCompact(Number(v)) },
    },
    y1: {
      position: 'right' as const,
      grid: { drawOnChartArea: false },
      ticks: { color: AMBER, font: { size: 10 }, callback: (v: number | string) => '$' + Number(v).toFixed(2) },
    },
  },
}

const engineChartData = computed(() => {
  const e = stats.value?.by_engine ?? []
  return {
    labels: e.map((x) => x.engine),
    datasets: [
      {
        data: e.map((x) => x.tokens),
        backgroundColor: e.map((_, i) => PALETTE[i % PALETTE.length]),
        borderWidth: 0,
      },
    ],
  }
})

const doughnutOptions = {
  responsive: true,
  maintainAspectRatio: false,
  cutout: '62%',
  plugins: {
    legend: { position: 'bottom' as const, labels: { color: tickColor, boxWidth: 12, font: { size: 11 } } },
  },
}

function pct(part: number, total: number): string {
  if (!total) return '0%'
  return ((part / total) * 100).toFixed(0) + '%'
}

// ── real subscription plan usage (read live from /usage during runs) ────────────
// The budget store only holds the single guardrail verdict, so Stats fetches the
// raw `/usage` snapshot itself for the detailed breakdown below.
interface PlanWindowData { utilization: number | null; resets_at: string | null }
interface PlanUsageData {
  subscription_type: string | null
  rate_limits_available: boolean
  windows: Record<'five_hour' | 'seven_day' | 'seven_day_opus' | 'seven_day_sonnet', PlanWindowData>
}
const planUsage = ref<PlanUsageData | null>(null)
// With managed accounts each one keeps its own snapshot, keyed by account id.
const planUsageByAccount = ref<Record<string, PlanUsageData | null>>({})
const planUsageCodex = ref<PlanUsageData | null>(null)
const now = ref(Date.now())
let unlistenPlanUsage: UnlistenFn | null = null
let clockTimer: ReturnType<typeof setInterval> | null = null

async function refreshPlanUsage() {
  if (!hasAccounts.value) {
    planUsage.value = await invoke<PlanUsageData | null>('get_plan_usage')
    return
  }
  const entries = await Promise.all(accountsStore.accounts.map(async (a) =>
    [a.id, await invoke<PlanUsageData | null>('get_plan_usage', { accountId: a.id })] as const))
  planUsageByAccount.value = Object.fromEntries(entries)
}
async function refreshPlanUsageCodex() {
  planUsageCodex.value = await invoke<PlanUsageData | null>('get_plan_usage_codex')
}

const PLAN_WINDOWS: { key: 'five_hour' | 'seven_day' | 'seven_day_opus' | 'seven_day_sonnet'; label: string }[] = [
  { key: 'five_hour', label: t('stats.plan.windowFiveHour') },
  { key: 'seven_day', label: t('stats.plan.windowSevenDay') },
  { key: 'seven_day_opus', label: t('stats.plan.windowSevenDayOpus') },
  { key: 'seven_day_sonnet', label: t('stats.plan.windowSevenDaySonnet') },
]
function planRowsFor(u: PlanUsageData | null, windows: typeof PLAN_WINDOWS) {
  if (!u || !u.rate_limits_available) return []
  return windows.map((w) => {
    const win = u.windows[w.key]
    return {
      label: w.label,
      utilization: win?.utilization ?? null,
      resets_at: win?.resets_at ?? null,
    }
  }).filter((r) => r.utilization != null)
}
// The i18n pieces are the sentence around the bold "real" and the slash command.
const claudePlanHint = computed(() =>
  `${t('stats.plan.claudeDescPrefix')} ${t('stats.plan.real')} ${t('stats.plan.claudeDescSuffix')} /usage ${t('stats.plan.claudeDescEnd')}`,
)
const codexPlanHint = computed(() =>
  `${t('stats.plan.codexDescPrefix')} ${t('stats.plan.real')} ${t('stats.plan.codexDescSuffix')} /status${t('stats.plan.codexDescEnd')}`,
)

// One block per managed account (default first); a single unlabeled block otherwise.
const claudePlanGroups = computed(() => {
  if (!hasAccounts.value) {
    return [{ key: 'global', label: null as string | null, isDefault: false, usage: planUsage.value, rows: planRowsFor(planUsage.value, PLAN_WINDOWS) }]
  }
  return [...accountsStore.accounts]
    .sort((a, b) => Number(b.is_default) - Number(a.is_default))
    .map((a) => {
      const usage = planUsageByAccount.value[a.id] ?? null
      return { key: a.id, label: a.label as string | null, isDefault: a.is_default, usage, rows: planRowsFor(usage, PLAN_WINDOWS) }
    })
})
// Codex only reports two windows: rolling 5h + weekly (no per-model split).
const CODEX_PLAN_WINDOWS: typeof PLAN_WINDOWS = [
  { key: 'five_hour', label: t('stats.plan.windowFiveHour') },
  { key: 'seven_day', label: t('stats.plan.windowCodexWeek') },
]
const planRowsCodex = computed(() => planRowsFor(planUsageCodex.value, CODEX_PLAN_WINDOWS))
function planResetText(iso: string | null): string {
  if (!iso) return ''
  const ms = new Date(iso).getTime() - now.value
  if (ms <= 0) return t('stats.plan.resetsSoon')
  const totalMinutes = Math.max(1, Math.ceil(ms / 60_000))
  const days = Math.floor(totalMinutes / 1440)
  const hours = Math.floor((totalMinutes % 1440) / 60)
  const minutes = totalMinutes % 60
  if (days > 0) return hours > 0 ? t('stats.plan.resetsInDaysHours', { days, hours }) : t('stats.plan.resetsInDays', { days })
  if (hours > 0) return minutes > 0 ? t('stats.plan.resetsInHoursMinutes', { hours, minutes }) : t('stats.plan.resetsInHours', { hours })
  return t('stats.plan.resetsInMinutes', { minutes })
}

// ── actions: backfill + reset ──────────────────────────────────────────────────
const backfilling = ref(false)
const actionMsg = ref<string | null>(null)

async function doBackfill() {
  backfilling.value = true
  actionMsg.value = null
  try {
    const r = await backfillUsage()
    actionMsg.value = t('stats.action.backfilled', { inserted: r.inserted, scanned: r.runs_scanned })
    await load()
  } catch (e) {
    actionMsg.value = t('stats.action.backfillFailed', { error: String(e) })
  } finally {
    backfilling.value = false
  }
}

const showReset = ref(false)
const resetConfirm = ref('')
const resetting = ref(false)

function openReset() {
  resetConfirm.value = ''
  showReset.value = true
}

async function doReset() {
  if (resetConfirm.value !== 'RESET') return
  resetting.value = true
  try {
    const r = await resetUsageStats()
    actionMsg.value = t('stats.action.resetDone', { runs: r.runs_deleted, usage: r.usage_cleared })
    showReset.value = false
    await load()
  } catch (e) {
    // Local-only: a remote desktop gets `remote_denied_local_only`.
    actionMsg.value = t('stats.action.resetFailed', { error: errorText(e, t) })
  } finally {
    resetting.value = false
  }
}

// ── storage: disk usage, and the cleanup of what nothing uses any more ─────────
const storage = ref<StorageStats | null>(null)
const storageLoading = ref(false)
const preview = ref<CleanupPreview | null>(null)
const previewLoading = ref(false)
const cleanupOpen = ref(false)
const compacting = ref(false)
// Cleaning and compacting are local-only: a remote desktop only looks.
const canClean = !isRemoteMode()
let unlistenCleaned: UnlistenFn | null = null
const { confirm } = useConfirm()

const cleanable = computed(() => mergeCleanable(storage.value?.categories.map((c) => c.id) ?? [], preview.value))

/** The i18n label of a row the cleanup added, else the backend's (the original rows). */
function rowText(c: { id: string; label: string; description: string }, part: 'label' | 'description'): string {
  const key = `storageCleanup.category.${c.id}.${part}`
  return te(key) ? t(key) : c[part]
}

// Each kind keeps items for its own reason; a kind with no reason of its own
// gets the plain count.
function keptText(id: string, n: number): string {
  const key = `storageCleanup.keptBy.${id}`
  return te(key) ? t(key, { n }) : t('storageCleanup.kept', { n })
}

function fmtBytes(n: number): string {
  if (n <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), units.length - 1)
  const v = n / Math.pow(1024, i)
  return `${v.toFixed(i === 0 ? 0 : v >= 100 ? 0 : 1)} ${units[i]}`
}

async function loadStorage() {
  storageLoading.value = true
  try {
    await Promise.all([
      getStorageStats().then((s) => { storage.value = s }),
      loadPreview(),
    ])
  } catch (e) {
    actionMsg.value = t('stats.action.storageReadFailed', { error: String(e) })
  } finally {
    storageLoading.value = false
  }
}

/** The "Cleanable" column and the dialog; a failure leaves that column empty. */
async function loadPreview() {
  previewLoading.value = true
  try {
    preview.value = await previewStorageCleanup()
  } catch (e) {
    actionMsg.value = t('storageCleanup.previewFailed', { error: errorText(e, t) })
  } finally {
    previewLoading.value = false
  }
}

function openCleanup() {
  cleanupOpen.value = true
  loadPreview()
}

// The table rescans on `storage:cleaned`.
function onCleaned(outcome: CleanupOutcome) {
  cleanupOpen.value = false
  const params = { size: fmtBytes(outcome.bytes), files: fmtNum(outcome.files), errors: outcome.errors }
  actionMsg.value = t(outcome.errors ? 'storageCleanup.doneWithErrors' : 'storageCleanup.done', params)
}

async function compact() {
  const ok = await confirm({
    title: t('storageCleanup.compactConfirm.title'),
    message: t('storageCleanup.compactConfirm.message'),
    confirmLabel: t('storageCleanup.compactConfirm.confirm'),
    variant: 'destructive',
  })
  if (!ok) return
  compacting.value = true
  try {
    const r = await compactDatabase()
    actionMsg.value = t('storageCleanup.compactDone', { before: fmtBytes(r.before_bytes), after: fmtBytes(r.after_bytes) })
    await loadStorage()
  } catch (e) {
    const code = String(e)
    actionMsg.value =
      code === 'runs_active' ? t('storageCleanup.compactRunsActive')
        : code === 'db_busy' ? t('storageCleanup.compactDbBusy')
          : code === 'busy' ? t('storageCleanup.busy')
            : t('storageCleanup.compactFailed', { error: errorText(e, t) })
  } finally {
    compacting.value = false
  }
}
</script>

<template>
  <div class="flex flex-col h-full">
    <!-- Header -->
    <PageHeader :title="t('stats.title')">
      <template #actions>
        <Button
          variant="outline"
          :disabled="backfilling"
          :title="t('stats.backfillTitle')"
          @click="doBackfill"
        >
          <Loader2 v-if="backfilling" class="h-3.5 w-3.5 animate-spin" />
          <RefreshCw v-else class="h-3.5 w-3.5" :stroke-width="1.75" />
          {{ t('stats.backfill') }}
        </Button>
        <Button
          variant="destructive"
          :title="t('stats.resetTitle')"
          @click="openReset"
        >
          <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
          {{ t('stats.reset') }}
        </Button>
      </template>
    </PageHeader>

    <!-- Body -->
    <div class="flex-1 overflow-auto p-6 space-y-5">
      <!-- Filters -->
      <div class="flex flex-wrap items-center gap-2">
        <AppSelect v-model="range" :options="RANGE_OPTIONS" class="w-[150px]" />
        <AppSelect v-model="engine" :options="engineOptions" class="w-[150px]" />
        <AppSelect v-model="projectId" :options="projectOptions" class="w-[150px]" />
        <AppSelect v-if="hasAccounts" v-model="accountId" :options="accountOptions" class="w-[150px]" />
        <span v-if="loading" class="flex items-center gap-1.5 text-xs text-muted-foreground">
          <Loader2 class="h-3.5 w-3.5 animate-spin" /> {{ t('common.loading') }}
        </span>
      </div>

      <p v-if="actionMsg" class="text-xs text-muted-foreground">{{ actionMsg }}</p>
      <Banner v-if="error" tone="destructive" live>{{ error }}
        <template #actions><Button variant="outline" size="xs" :disabled="loading" @click="load">{{ t('common.retry') }}</Button></template>
      </Banner>

      <!-- Real subscription plan usage (independent of token stats) -->
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <!-- Claude subscription plan usage (from /usage) -->
        <Card body-class="px-4 py-3.5 space-y-3">
          <div>
            <h2 class="flex items-center gap-1.5 text-md font-semibold text-foreground">
              <Gauge class="h-3.5 w-3.5" :stroke-width="1.75" /> {{ t('stats.plan.claudeTitle') }}
              <InfoTip :text="claudePlanHint" />
            </h2>
          </div>
          <div v-for="g in claudePlanGroups" :key="g.key" class="space-y-2">
            <p v-if="g.label" class="flex items-center gap-1.5 text-2xs font-semibold">
              {{ g.label }}
              <span v-if="g.isDefault" class="rounded bg-muted px-1 py-px text-2xs font-normal text-muted-foreground">{{ t('stats.account.default') }}</span>
            </p>
            <div v-if="g.rows.length" class="space-y-2">
              <div v-for="row in g.rows" :key="row.label" class="space-y-1">
                <div class="flex items-baseline justify-between text-2xs">
                  <span class="font-medium">{{ row.label }}</span>
                  <span class="font-mono text-muted-foreground">
                    {{ Math.round(row.utilization!) }}%
                    <span v-if="planResetText(row.resets_at)"> · {{ planResetText(row.resets_at) }}</span>
                  </span>
                </div>
                <div class="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                  <div
                    class="h-full rounded-full transition-all"
                    :class="row.utilization! >= 100 ? 'bg-destructive' : row.utilization! >= 80 ? 'bg-warning' : 'bg-info'"
                    :style="{ width: Math.min(100, Math.round(row.utilization!)) + '%' }"
                  />
                </div>
              </div>
              <p v-if="g.usage?.subscription_type" class="text-2xs text-muted-foreground">
                {{ t('stats.plan.label') }} <b class="capitalize">{{ g.usage.subscription_type }}</b>
              </p>
            </div>
            <p v-else class="text-2xs text-muted-foreground italic">
              {{ g.label ? t('stats.plan.emptyAccount') : t('stats.plan.empty') }}
            </p>
          </div>
        </Card>

        <!-- Codex subscription plan usage (from account/rateLimits) -->
        <Card body-class="px-4 py-3.5 space-y-3">
          <div>
            <h2 class="flex items-center gap-1.5 text-md font-semibold text-foreground">
              <Gauge class="h-3.5 w-3.5" :stroke-width="1.75" /> {{ t('stats.plan.codexTitle') }}
              <InfoTip :text="codexPlanHint" />
            </h2>
          </div>
          <div v-if="planRowsCodex.length" class="space-y-2">
            <div v-for="row in planRowsCodex" :key="row.label" class="space-y-1">
              <div class="flex items-baseline justify-between text-2xs">
                <span class="font-medium">{{ row.label }}</span>
                <span class="font-mono text-muted-foreground">
                  {{ Math.round(row.utilization!) }}%
                  <span v-if="planResetText(row.resets_at)"> · {{ planResetText(row.resets_at) }}</span>
                </span>
              </div>
              <div class="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                <div
                  class="h-full rounded-full transition-all"
                  :class="row.utilization! >= 100 ? 'bg-destructive' : row.utilization! >= 80 ? 'bg-warning' : 'bg-info'"
                  :style="{ width: Math.min(100, Math.round(row.utilization!)) + '%' }"
                />
              </div>
            </div>
            <p v-if="planUsageCodex?.subscription_type" class="text-2xs text-muted-foreground">
              {{ t('stats.plan.label') }} <b class="capitalize">{{ planUsageCodex.subscription_type }}</b>
            </p>
          </div>
          <p v-else class="text-2xs text-muted-foreground italic">
            {{ t('stats.plan.emptyCodex') }}
          </p>
        </Card>
      </div>

      <!-- Empty state -->
      <div
        v-if="!loading && !hasData"
        class="flex flex-col items-center justify-center py-20 text-center"
      >
        <Coins class="h-8 w-8 text-muted-foreground/40 mb-3" :stroke-width="1.5" />
        <p class="text-sm text-muted-foreground">{{ t('stats.empty.title') }}</p>
        <p class="text-xs text-muted-foreground/70 mt-1">
          {{ t('stats.empty.hint') }}
        </p>
      </div>

      <template v-if="hasData && stats">
        <!-- Summary cards -->
        <div class="grid grid-cols-2 lg:grid-cols-4 gap-3">
          <Card body-class="px-3.5 py-3">
            <div class="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
              <Coins class="h-3.5 w-3.5" :stroke-width="1.75" /> {{ t('stats.summary.totalTokens') }}
            </div>
            <p class="text-stat font-semibold tabular-nums">{{ fmtNum(stats.summary.total_tokens) }}</p>
            <p class="text-2xs text-muted-foreground/70 mt-1">
              {{ t('stats.summary.tokensBreakdown', { input: fmtCompact(stats.summary.total_input), output: fmtCompact(stats.summary.total_output), cache: fmtCompact(stats.summary.total_cache) }) }}
            </p>
          </Card>
          <Card body-class="px-3.5 py-3">
            <div class="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
              <DollarSign class="h-3.5 w-3.5" :stroke-width="1.75" /> {{ t('stats.summary.estCost') }}
              <span :title="t('stats.summary.estCostTooltip')">
                <Info class="h-3 w-3 text-muted-foreground/50" />
              </span>
            </div>
            <p class="text-stat font-semibold tabular-nums">{{ fmtCost(stats.summary.total_cost) }}</p>
            <p class="text-2xs text-muted-foreground/70 mt-1">
              {{ t('stats.summary.estimated', { cost: fmtCost(stats.summary.estimated_cost) }) }}
            </p>
          </Card>
          <Card body-class="px-3.5 py-3">
            <div class="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
              <Play class="h-3.5 w-3.5" :stroke-width="1.75" /> {{ t('stats.summary.runs') }}
            </div>
            <p class="text-stat font-semibold tabular-nums">{{ fmtNum(stats.summary.total_runs) }}</p>
          </Card>
          <Card body-class="px-3.5 py-3">
            <div class="flex items-center gap-1.5 text-xs text-muted-foreground mb-1">
              <Repeat class="h-3.5 w-3.5" :stroke-width="1.75" /> {{ t('stats.summary.turns') }}
            </div>
            <p class="text-stat font-semibold tabular-nums">{{ fmtNum(stats.summary.total_turns) }}</p>
          </Card>
        </div>

        <!-- Daily chart -->
        <Card body-class="p-4">
          <h2 class="text-xs font-medium text-muted-foreground mb-3">{{ t('stats.charts.dailyTitle') }}</h2>
          <div class="h-64">
            <KeyboardChart
              :data="dailyChartData"
              :options="dailyChartOptions"
              :describe="describeDay"
              :label="t('stats.charts.dailyAria')"
            />
          </div>
        </Card>

        <!-- Engine + project breakdown -->
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
          <Card body-class="p-4">
            <h2 class="text-xs font-medium text-muted-foreground mb-3">{{ t('stats.charts.byEngine') }}</h2>
            <div class="h-52">
              <Doughnut :data="engineChartData" :options="doughnutOptions" />
            </div>
            <table class="w-full mt-3 text-xs">
              <tbody>
                <tr v-for="(e, i) in stats.by_engine" :key="e.engine" class="border-t border-border/40">
                  <td class="py-1.5">
                    <span class="inline-block h-2.5 w-2.5 rounded-sm mr-1.5 align-middle"
                      :style="{ background: PALETTE[i % PALETTE.length] }" />
                    {{ e.engine }}
                  </td>
                  <td class="py-1.5 text-right tabular-nums">{{ fmtNum(e.tokens) }}</td>
                  <td class="py-1.5 text-right tabular-nums text-muted-foreground">{{ fmtCost(e.cost) }}</td>
                </tr>
              </tbody>
            </table>
          </Card>

          <Card body-class="p-4">
            <h2 class="text-xs font-medium text-muted-foreground mb-3">{{ t('stats.charts.byProject') }}</h2>
            <table class="w-full text-xs">
              <thead>
                <tr class="text-muted-foreground/70 text-left">
                  <th class="font-normal pb-2">{{ t('stats.table.project') }}</th>
                  <th class="font-normal pb-2 text-right">{{ t('stats.table.tokens') }}</th>
                  <th class="font-normal pb-2 text-right">{{ t('stats.table.cost') }}</th>
                  <th class="font-normal pb-2 text-right">{{ t('stats.table.percent') }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="p in stats.by_project" :key="p.project_id ?? 'none'" class="border-t border-border/40">
                  <td class="py-1.5 truncate max-w-[160px]">{{ p.project_name ?? t('stats.table.deletedProject') }}</td>
                  <td class="py-1.5 text-right tabular-nums">{{ fmtNum(p.tokens) }}</td>
                  <td class="py-1.5 text-right tabular-nums text-muted-foreground">{{ fmtCost(p.cost) }}</td>
                  <td class="py-1.5 text-right tabular-nums text-muted-foreground">
                    {{ pct(p.tokens, stats.summary.total_tokens) }}
                  </td>
                </tr>
              </tbody>
            </table>
          </Card>
        </div>

        <!-- By Claude account -->
        <Card v-if="hasAccounts && stats.by_account.length" body-class="p-4">
          <h2 class="text-xs font-medium text-muted-foreground mb-3">{{ t('stats.table.byAccount') }}</h2>
          <table class="w-full text-xs">
            <thead>
              <tr class="text-muted-foreground/70 text-left">
                <th class="font-normal pb-2">{{ t('stats.table.account') }}</th>
                <th class="font-normal pb-2 text-right">{{ t('stats.table.tokens') }}</th>
                <th class="font-normal pb-2 text-right">{{ t('stats.table.cost') }}</th>
                <th class="font-normal pb-2 text-right">{{ t('stats.table.runs') }}</th>
                <th class="font-normal pb-2 text-right">{{ t('stats.table.percent') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="a in stats.by_account" :key="a.account_id ?? 'none'" class="border-t border-border/40">
                <td class="py-1.5 truncate max-w-[200px]">{{ accountName(a) }}</td>
                <td class="py-1.5 text-right tabular-nums">{{ fmtNum(a.tokens) }}</td>
                <td class="py-1.5 text-right tabular-nums text-muted-foreground">{{ fmtCost(a.cost) }}</td>
                <td class="py-1.5 text-right tabular-nums text-muted-foreground">{{ fmtNum(a.runs) }}</td>
                <td class="py-1.5 text-right tabular-nums text-muted-foreground">
                  {{ pct(a.tokens, stats.summary.total_tokens) }}
                </td>
              </tr>
            </tbody>
          </table>
        </Card>

        <!-- By model -->
        <Card body-class="p-4">
          <h2 class="text-xs font-medium text-muted-foreground mb-3">{{ t('stats.table.byModel') }}</h2>
          <table class="w-full text-xs">
            <thead>
              <tr class="text-muted-foreground/70 text-left">
                <th class="font-normal pb-2">{{ t('stats.table.model') }}</th>
                <th class="font-normal pb-2 text-right">{{ t('stats.table.tokens') }}</th>
                <th class="font-normal pb-2 text-right">{{ t('stats.table.cost') }}</th>
                <th class="font-normal pb-2 text-right">{{ t('stats.table.runs') }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="m in stats.by_model" :key="m.model ?? 'unknown'" class="border-t border-border/40">
                <td class="py-1.5 font-mono">{{ m.model ?? t('stats.table.unknownModel') }}</td>
                <td class="py-1.5 text-right tabular-nums">{{ fmtNum(m.tokens) }}</td>
                <td class="py-1.5 text-right tabular-nums text-muted-foreground">{{ fmtCost(m.cost) }}</td>
                <td class="py-1.5 text-right tabular-nums text-muted-foreground">{{ fmtNum(m.runs) }}</td>
              </tr>
            </tbody>
          </table>
        </Card>
      </template>

      <!-- Storage: disk usage, and the cleanup of old files nothing uses (independent of token data) -->
      <Card body-class="p-4">
        <div class="flex items-center justify-between mb-3">
          <h2 class="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
            <HardDrive class="h-3.5 w-3.5" :stroke-width="1.75" /> {{ t('stats.storage.title') }}
            <span v-if="storage" class="text-muted-foreground/60">
              {{ t('stats.storage.totalSuffix', { bytes: fmtBytes(storage.total_bytes) }) }}
            </span>
            <InfoTip :text="t('stats.storage.note')" />
          </h2>
          <div class="flex items-center gap-1">
            <template v-if="canClean">
              <Button variant="ghost" size="sm" :disabled="compacting" @click="compact">
                <Loader2 v-if="compacting" class="h-3.5 w-3.5 animate-spin" />
                <Database v-else class="h-3.5 w-3.5" :stroke-width="1.75" />
                {{ t('storageCleanup.compact') }}
              </Button>
              <Button variant="outline" size="sm" @click="openCleanup">
                <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
                {{ t('storageCleanup.cleanNow') }}
              </Button>
            </template>
            <Button variant="ghost" size="sm" :disabled="storageLoading" @click="loadStorage">
              <Loader2 v-if="storageLoading" class="h-3.5 w-3.5 animate-spin" />
              <RefreshCw v-else class="h-3.5 w-3.5" :stroke-width="1.75" />
              {{ t('stats.storage.rescan') }}
            </Button>
          </div>
        </div>

        <div v-if="!storage && storageLoading" class="text-xs text-muted-foreground py-4">
          {{ t('stats.storage.scanning') }}
        </div>

        <table v-else-if="storage" class="w-full text-xs">
          <thead>
            <tr class="text-muted-foreground/70 text-left">
              <th class="font-normal pb-2">{{ t('stats.storage.category') }}</th>
              <th class="font-normal pb-2 text-right">{{ t('stats.storage.files') }}</th>
              <th class="font-normal pb-2 text-right">{{ t('stats.storage.size') }}</th>
              <th class="font-normal pb-2 text-right">{{ t('stats.storage.percent') }}</th>
              <th class="font-normal pb-2 text-right" :title="preview ? t('storageCleanup.cleanableHint', { days: preview.effective_days }) : undefined">
                {{ t('storageCleanup.cleanable') }}
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="c in storage.categories" :key="c.id" class="border-t border-border/40">
              <td class="py-2" :class="c.subset && 'pl-4 text-muted-foreground'">
                <div class="flex items-center gap-1.5">
                  <span>{{ rowText(c, 'label') }}</span>
                  <span :title="rowText(c, 'description')">
                    <Info class="h-3 w-3 text-muted-foreground/50" />
                  </span>
                </div>
                <p class="font-mono text-2xs text-muted-foreground/60 mt-0.5">{{ c.path }}</p>
              </td>
              <td class="py-2 text-right tabular-nums text-muted-foreground">{{ fmtNum(c.file_count) }}</td>
              <td class="py-2 text-right tabular-nums">{{ fmtBytes(c.size_bytes) }}</td>
              <td class="py-2 text-right tabular-nums text-muted-foreground">
                {{ c.subset ? '' : pct(c.size_bytes, storage.total_bytes) }}
              </td>
              <td class="py-2 text-right tabular-nums">
                <template v-if="cleanable[c.id]">
                  <span :class="!cleanable[c.id]!.bytes && 'text-muted-foreground/60'">
                    {{ ESTIMATED.has(c.id) && cleanable[c.id]!.bytes
                      ? t('storageCleanup.about', { size: fmtBytes(cleanable[c.id]!.bytes) })
                      : fmtBytes(cleanable[c.id]!.bytes) }}
                  </span>
                  <p v-if="cleanable[c.id]!.keptItems" class="text-2xs text-warning mt-0.5">
                    {{ keptText(c.id, cleanable[c.id]!.keptItems) }}
                  </p>
                </template>
                <span v-else class="text-muted-foreground/40">—</span>
              </td>
            </tr>
          </tbody>
        </table>
      </Card>
      <StorageCleanupDialog
        :open="cleanupOpen"
        :preview="preview"
        :loading="previewLoading"
        @close="cleanupOpen = false"
        @cleaned="onCleaned"
      />
    </div>

    <!-- Reset confirm modal -->
    <div
      v-if="showReset"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      @click.self="showReset = false"
    >
      <div class="w-[420px] rounded-lg border border-border bg-card p-5 shadow-xl">
        <div class="flex items-center gap-2 text-destructive mb-3">
          <AlertTriangle class="h-5 w-5" />
          <h2 class="text-sm font-semibold">{{ t('stats.resetModal.title') }}</h2>
        </div>
        <p class="text-xs text-muted-foreground leading-relaxed mb-1">
          {{ t('stats.resetModal.line1') }}
        </p>
        <p class="text-xs text-destructive/90 mb-4">{{ t('stats.resetModal.irreversible') }}</p>
        <label class="block text-xs text-muted-foreground mb-1.5">{{ t('stats.resetModal.typeToConfirm', { word: 'RESET' }) }}</label>
        <Input
          v-model="resetConfirm"
          type="text"
          size="sm"
          class="mb-4"
          placeholder="RESET"
          @keyup.enter="doReset"
        />
        <div class="flex justify-end gap-2">
          <Button
            variant="ghost"
            @click="showReset = false"
          >
            {{ t('common.cancel') }}
          </Button>
          <Button
            variant="destructive"
            :disabled="resetConfirm !== 'RESET' || resetting"
            @click="doReset"
          >
            <Loader2 v-if="resetting" class="h-3.5 w-3.5 animate-spin" />
            <Trash2 v-else class="h-3.5 w-3.5" />
            {{ t('stats.resetModal.resetEverything') }}
          </Button>
        </div>
      </div>
    </div>
  </div>
</template>
