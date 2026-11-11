<script setup lang="ts">
// Plan usage on the session status bar (UX-23, replaces BudgetBadge): one
// UsageRing per Claude account (or the single global Claude snapshot when no
// accounts are set up), the shared Codex login and each Codex account (UX-41).
// A ring hides while its account has no plan data. This component also keeps
// the numbers fresh and raises the quota warning toast (UX-13), so both stop
// while the bar hides usage.
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@/lib/tauri'
import UsageRing from '@/components/UsageRing.vue'
import { useBudgetStore } from '@/stores/budget'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { useCodexAccountsStore } from '@/stores/codexAccounts'
import { useToast } from '@/composables/useToast'
import type { QuotaWarnWindow } from '@/lib/quotaWarn'
import {
  parsePercentSetting, planUsageRead, usageWindows,
  type PlanUsageSnapshot, type UsageRingAccount, type UsageThresholds,
} from '@/lib/usageRing'

const { t } = useI18n()
const budget = useBudgetStore()
const app = useAppSettingsStore()
const claudeAccounts = useClaudeAccountsStore()
const codexAccounts = useCodexAccountsStore()
const { toast } = useToast()
const now = ref(Date.now())

// Full plan snapshots by row key. The budget verdicts only carry one window;
// the card lists every window, so the rings read the snapshots themselves.
const snapshots = ref<Record<string, PlanUsageSnapshot | null>>({})

/** One usage row before it becomes a ring: the snapshot plus its budget verdict. */
interface UsageRow extends UsageRingAccount {
  /** `undefined` until the verdict loads (the quota toast then skips the row). */
  quotaWarnWindows: QuotaWarnWindow[] | undefined
  refresh: () => Promise<void>
}

const rows = computed<UsageRow[]>(() => {
  const claudeRows: UsageRow[] = claudeAccounts.accounts.length > 0
    ? claudeAccounts.accounts.map((acc) => {
      const key = `claude:${acc.id}`
      const b = claudeAccounts.budgets[acc.id]
      return {
        key,
        label: acc.label,
        engine: 'Claude',
        windows: usageWindows(snapshots.value[key], now.value),
        capturedAt: snapshots.value[key]?.captured_at ?? null,
        refreshing: claudeAccounts.refreshingUsage[acc.id] ?? false,
        refreshError: claudeAccounts.usageErrors[acc.id] ?? null,
        quotaWarnWindows: b ? (b.quota_warn_windows ?? []) : undefined,
        refresh: async () => {
          await claudeAccounts.refreshUsage(acc.id)
          await loadSnapshot(key)
        },
      }
    })
    : [{
      key: 'claude',
      label: 'Claude',
      engine: 'Claude',
      windows: usageWindows(snapshots.value.claude, now.value),
      capturedAt: snapshots.value.claude?.captured_at ?? null,
      refreshing: budget.refreshingPlan,
      refreshError: budget.refreshError,
      quotaWarnWindows: budget.status ? (budget.quotaWarnWindows ?? []) : undefined,
      refresh: async () => {
        await budget.refreshPlanUsage({ reason: 'manual', force: true })
        await loadSnapshot('claude')
      },
    }]
  const codexAccountRows: UsageRow[] = codexAccounts.accounts.map((acc) => {
    const key = `codex:${acc.id}`
    const b = codexAccounts.budgets[acc.id]
    return {
      key,
      label: acc.label,
      engine: 'Codex',
      windows: usageWindows(snapshots.value[key], now.value),
      capturedAt: snapshots.value[key]?.captured_at ?? null,
      refreshing: codexAccounts.refreshingUsage[acc.id] ?? false,
      refreshError: codexAccounts.usageErrors[acc.id] ?? null,
      quotaWarnWindows: b ? (b.quota_warn_windows ?? []) : undefined,
      refresh: async () => {
        await codexAccounts.refreshUsage(acc.id)
        await loadSnapshot(key)
      },
    }
  })
  return [...claudeRows, {
    key: 'codex',
    // With Codex accounts, this ring is the shared `~/.codex` login's only.
    label: codexAccounts.accounts.length > 0 ? t('settings.accounts.sharedCodex') : 'Codex',
    engine: 'Codex',
    windows: usageWindows(snapshots.value.codex, now.value),
    capturedAt: snapshots.value.codex?.captured_at ?? null,
    refreshing: budget.refreshingCodexPlan,
    refreshError: budget.codexRefreshError,
    quotaWarnWindows: budget.codexStatus ? (budget.codexQuotaWarnWindows ?? []) : undefined,
    refresh: async () => {
      await budget.refreshCodexPlanUsage({ reason: 'manual', force: true })
      await loadSnapshot('codex')
    },
  }, ...codexAccountRows]
})

const visibleRows = computed(() => rows.value.filter((r) => r.windows.length > 0))

const thresholds = computed<UsageThresholds>(() => ({
  warnPercent: parsePercentSetting(app.settings?.quota_warn_percent),
  block5hPercent: parsePercentSetting(app.settings?.budget_5h_percent),
  blockWeekPercent: parsePercentSetting(app.settings?.budget_week_percent),
}))

// Quota warning tier: one toast per window (5h / weekly) and row when it first
// reaches `quota_warn_percent`; again only after it dropped below or reset. The
// latch lives in the budget store, so a remount on a project switch doesn't
// warn again for the same crossing.
const WINDOW_LABEL = computed<Record<string, string>>(() => ({
  '5h': t('misc.budget.window5h'),
  week: t('misc.budget.windowWeek'),
}))
watch(() => rows.value.map((r) => ({ key: r.key, label: r.label, windows: r.quotaWarnWindows })), (list) => {
  const crossings = budget.takeQuotaWarnings(list.map((r) => ({ key: r.key, windows: r.windows })))
  for (const c of crossings) {
    const label = list.find((r) => r.key === c.key)?.label ?? c.key
    toast.info(t('misc.budget.quotaWarnToast', {
      label, percent: c.percent, window: WINDOW_LABEL.value[c.period] ?? c.period,
    }), 8000)
  }
}, { immediate: true })

/** Snapshot reads are local DB reads (no engine call). Keeps the old value on error. */
async function loadSnapshot(key: string) {
  try {
    const read = planUsageRead(key)
    snapshots.value[key] = await invoke<PlanUsageSnapshot | null>(read.cmd, read.args)
  } catch {
    // leave the previous value on transient errors
  }
}
async function loadSnapshots() {
  await Promise.all(rows.value.map((r) => loadSnapshot(r.key)))
}

/** Re-read every managed Claude account's verdict (skips accounts mid-probe). */
async function refreshAccountBudgets() {
  await Promise.all(
    claudeAccounts.accounts
      .filter((a) => !claudeAccounts.refreshingUsage[a.id])
      .map((a) => claudeAccounts.fetchBudget(a.id)),
  )
}

/** Same for every Codex account. */
async function refreshCodexAccountBudgets() {
  await Promise.all(
    codexAccounts.accounts
      .filter((a) => !codexAccounts.refreshingUsage[a.id])
      .map((a) => codexAccounts.fetchBudget(a.id)),
  )
}

const isCodexRow = (key: string) => key === 'codex' || key.startsWith('codex:')

/** Re-read the Claude rows: account verdicts and snapshots (local reads). */
function reloadClaude() {
  void refreshAccountBudgets()
  void Promise.all(rows.value.filter((r) => !isCodexRow(r.key)).map((r) => loadSnapshot(r.key)))
}

/** Re-read the Codex rows: the shared login and every account. */
function reloadCodex() {
  if (!budget.refreshingCodexPlan) void budget.refreshCodex()
  void refreshCodexAccountBudgets()
  void Promise.all(rows.value.filter((r) => isCodexRow(r.key)).map((r) => loadSnapshot(r.key)))
}

/** Local reads only: the verdicts (quota toast) and the snapshots (rings). */
function reloadAll() {
  void budget.refresh()
  void budget.refreshCodex()
  void refreshAccountBudgets()
  void refreshCodexAccountBudgets()
  void loadSnapshots()
}

let timer: ReturnType<typeof setInterval> | null = null
let clockTimer: ReturnType<typeof setInterval> | null = null
let unlistenPlanUsage: UnlistenFn | null = null
let unlistenBudgetStatus: UnlistenFn | null = null
let unmounted = false

onMounted(async () => {
  await app.ensureLoaded()
  now.value = Date.now()
  await Promise.all([budget.refresh(), budget.refreshCodex(), claudeAccounts.fetch(), codexAccounts.fetch()])
  await loadSnapshots()
  if (unmounted) return
  // The only automatic probes: one per provider per app session. After that the
  // % stays fresh from the capture on every run, or the card's refresh button.
  // The store's startup flag (not a component flag) guards them, because this
  // bar mounts again on every project switch and a probe shells out to the
  // `claude` / `codex` CLI. The Claude probe runs on the default account, the
  // Codex one on the shared `~/.codex` login.
  void budget.refreshPlanUsage({ reason: 'startup' }).then(reloadClaude)
  void budget.refreshCodexPlanUsage({ reason: 'startup' }).then(() => loadSnapshot('codex'))
  // Recompute rollover against the clock and pick up snapshots written by runs.
  timer = setInterval(reloadAll, 60_000)
  clockTimer = setInterval(() => { now.value = Date.now() }, 30_000)
  // Immediately when a sidecar / watcher persists a fresh snapshot during a run.
  const offPlan = await listen<{ provider?: string }>('plan_usage_updated', (e) => {
    if (e.payload?.provider === 'codex') {
      reloadCodex()
    } else {
      if (!budget.refreshingPlan) void budget.refresh()
      reloadClaude()
    }
  })
  const offBudget = await listen('budget_status_updated', reloadAll)
  // Unmounted while awaiting the listeners (fast project switch): drop them now.
  if (unmounted) {
    offPlan()
    offBudget()
    return
  }
  unlistenPlanUsage = offPlan
  unlistenBudgetStatus = offBudget
})

onUnmounted(() => {
  unmounted = true
  if (timer) clearInterval(timer)
  if (clockTimer) clearInterval(clockTimer)
  if (unlistenPlanUsage) unlistenPlanUsage()
  if (unlistenBudgetStatus) unlistenBudgetStatus()
})
</script>

<template>
  <div v-if="visibleRows.length > 0" class="flex min-w-0 items-center gap-0.5 overflow-x-auto overflow-y-hidden">
    <UsageRing
      v-for="r in visibleRows"
      :key="r.key"
      :account="r"
      :thresholds="thresholds"
      :now="now"
      @refresh="r.refresh"
    />
  </div>
</template>
