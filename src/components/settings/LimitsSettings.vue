<script setup lang="ts">
// Settings → AI › "Limits" (was "Sessions", V3-60): the context meter (warning / limit override) and
// the usage quota tiers — a warning (`quota_warn_percent`, toast + badge colour)
// and the blocks on new sessions / follow-ups (`budget_5h_percent`,
// `budget_week_percent`, enforced by `stats::enforce_budget`), plus the opt-in
// stop of running sessions at those thresholds (`quota_stop_running`,
// `runs::quota_stop`) — and the limits on a turn nobody watches
// (`unattended_max_turns`, `unattended_max_minutes`, `runs::unattended`),
// and the desktop notification switches (`desktop_notify`, NotificationSettings).
// Under each block threshold a "Now: …" line shows every account's current %
// (lib/quotaNow), read from the snapshots the usage rings use.
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Gauge } from 'lucide-vue-next'
import { Card, InfoTip, Input, SegmentedControl, ToggleSwitch, type SegmentedOption } from '@/components/ui'
import { useSettingsForm } from './useSettingsForm'
import SettingsRow from './SettingsRow.vue'
import NotificationSettings from './NotificationSettings.vue'
import { usePlanSnapshots } from '@/composables/usePlanSnapshots'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { useCodexAccountsStore } from '@/stores/codexAccounts'
import { quotaNowLine, type QuotaNowRow } from '@/lib/quotaNow'
import { usageWindows } from '@/lib/usageRing'

const { t } = useI18n()
const { settings } = useSettingsForm()
const claudeAccounts = useClaudeAccountsStore()
const codexAccounts = useCodexAccountsStore()
const { snapshots, loadSnapshots } = usePlanSnapshots()
const now = ref(Date.now())

const pct = (v: string | number | undefined) => Number(String(v ?? '').trim()) || 0
const blocks = computed(() => [settings.value.budget_5h_percent, settings.value.budget_week_percent].map(pct))

// A warning at or above a set block threshold never shows before that block.
const warnNotBelowBlock = computed(() => {
  const warn = pct(settings.value.quota_warn_percent)
  if (warn <= 0) return false
  return blocks.value.some((block) => block > 0 && warn >= block)
})

// Stopping acts on the block thresholds, so it needs one of them set.
const hasBlock = computed(() => blocks.value.some((block) => block > 0))
const stopRunning = computed(() => settings.value.quota_stop_running === 'true')

// The accounts of the usage rings (Claude accounts, or the one global Claude
// login; the shared Codex login; Codex accounts), each with its snapshot.
const snapshotKeys = computed(() => [
  ...(claudeAccounts.accounts.length ? claudeAccounts.accounts.map((a) => `claude:${a.id}`) : ['claude']),
  'codex',
  ...codexAccounts.accounts.map((a) => `codex:${a.id}`),
])
const nowRows = computed<QuotaNowRow[]>(() => {
  const labels: Record<string, string> = { claude: 'Claude', codex: codexAccounts.accounts.length ? t('settings.accounts.sharedCodex') : 'Codex' }
  for (const a of claudeAccounts.accounts) labels[`claude:${a.id}`] = a.label
  for (const a of codexAccounts.accounts) labels[`codex:${a.id}`] = a.label
  return snapshotKeys.value.map((key) => ({
    label: labels[key],
    windows: usageWindows(snapshots.value[key], now.value),
    capturedAt: snapshots.value[key]?.captured_at ?? null,
  }))
})
const nowFor = (period: '5h' | 'week') => quotaNowLine(nowRows.value, period, now.value)
const now5h = computed(() => nowFor('5h'))
const nowWeek = computed(() => nowFor('week'))

let clock: ReturnType<typeof setInterval> | null = null
onMounted(async () => {
  await Promise.all([claudeAccounts.fetch(), codexAccounts.fetch()])
  now.value = Date.now()
  await loadSnapshots(snapshotKeys.value)
  clock = setInterval(() => { now.value = Date.now() }, 30_000)
})
onUnmounted(() => { if (clock) clearInterval(clock) })

// Sub-tabs keep this section one screen tall. The choice is not saved.
export type LimitsTab = 'context' | 'quota' | 'unattended' | 'notify'
type Tab = LimitsTab
const tab = defineModel<Tab>('tab', { default: 'context' })
const tabOptions = computed<SegmentedOption<Tab>[]>(() => [
  { value: 'context', label: t('settings.sessions.contextTitle') },
  { value: 'quota', label: t('settings.sessions.quotaTitle') },
  { value: 'unattended', label: t('settings.sessions.unattendedTitle') },
  { value: 'notify', label: t('settings.sessions.notifyTitle') },
])
const tabHint = computed(() => ({
  context: t('settings.sessions.contextHint'),
  quota: t('settings.sessions.quotaHint'),
  unattended: `${t('settings.sessions.unattendedHint')}\n\n${t('settings.sessions.unattendedStop')}`,
  notify: t('settings.sessions.notifyHint'),
})[tab.value])
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Gauge class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.sessions.title') }}</span>
    </template>

      <div class="flex items-center gap-1.5">
        <SegmentedControl v-model="tab" :options="tabOptions" :stretch="false" />
        <InfoTip :text="tabHint" />
      </div>

      <!-- Context window meter -->
      <template v-if="tab === 'context'">
        <SettingsRow search-id="limits-warnAt" :label="t('settings.sessions.warnAt')">
          <Input v-model="settings.context_warn_percent" type="number" min="1" max="100" placeholder="80" />
        </SettingsRow>
        <SettingsRow search-id="limits-limitOverride" :label="t('settings.sessions.limitOverride')" :hint="t('settings.sessions.limitOverrideHint')">
          <Input v-model="settings.context_limit_override" type="number" min="0" :placeholder="t('settings.sessions.limitOverridePlaceholder')" />
        </SettingsRow>
      </template>

      <!-- Desktop notification switches -->
      <div v-else-if="tab === 'notify'" class="space-y-4" data-settings-id="limits-notify">
        <NotificationSettings />
      </div>

      <!-- Limits on a turn nobody watches: a schedule's, or one Remote Control started -->
      <template v-else-if="tab === 'unattended'">
        <SettingsRow search-id="limits-unattendedMaxTurns" :label="t('settings.sessions.unattendedMaxTurns')" :hint="t('settings.sessions.unattendedMaxTurnsHint')">
          <Input v-model="settings.unattended_max_turns" type="number" min="0" placeholder="200" />
        </SettingsRow>
        <SettingsRow search-id="limits-unattendedMaxMinutes" :label="t('settings.sessions.unattendedMaxMinutes')" :hint="t('settings.sessions.unattendedMaxMinutesHint')">
          <Input v-model="settings.unattended_max_minutes" type="number" min="0" placeholder="60" />
        </SettingsRow>
      </template>

      <!-- Usage quota: a warning tier, then the blocks on new sessions / follow-ups -->
      <template v-else>
        <SettingsRow
          search-id="limits-quotaWarnAt" :label="t('settings.sessions.quotaWarnAt')"
          :hint="`${t('settings.sessions.quotaWarnHint')}\n\n${t('settings.sessions.orderHint')}`"
        >
          <Input v-model="settings.quota_warn_percent" type="number" min="1" max="100" :placeholder="t('settings.sessions.emptyOff')" />
        </SettingsRow>
        <SettingsRow search-id="limits-block5h" :label="t('settings.sessions.block5h')" :hint="t('settings.sessions.blockHint')">
          <Input v-model="settings.budget_5h_percent" type="number" min="1" max="100" :placeholder="t('settings.sessions.emptyOff')" />
        </SettingsRow>
        <SettingsRow search-id="limits-blockWeekly" :label="t('settings.sessions.blockWeekly')" :hint="t('settings.sessions.blockHint')">
          <Input v-model="settings.budget_week_percent" type="number" min="1" max="100" :placeholder="t('settings.sessions.emptyOff')" />
        </SettingsRow>
        <p v-if="warnNotBelowBlock" class="text-2xs text-warning leading-relaxed">
          {{ t('settings.sessions.warnNotBelowBlock') }}
        </p>
        <div class="space-y-1.5">
          <SettingsRow search-id="limits-stopRunning" :label="t('settings.sessions.stopRunning')" :hint="t('settings.sessions.stopRunningHint')">
            <div class="flex min-[901px]:justify-end">
              <ToggleSwitch
                size="md"
                :checked="stopRunning"
                :disabled="!hasBlock && !stopRunning"
                :label="t('settings.sessions.stopRunning')"
                @toggle="settings.quota_stop_running = stopRunning ? 'false' : 'true'"
              />
            </div>
          </SettingsRow>
          <!-- Kept on screen: stopping mid-turn has a cost the user should see before turning it on. -->
          <p class="text-2xs leading-relaxed" :class="stopRunning ? 'text-warning' : 'text-muted-foreground'">{{ t('settings.sessions.stopRunningWarning') }}</p>
          <p v-if="!hasBlock" class="text-2xs text-muted-foreground leading-relaxed">{{ t('settings.sessions.stopRunningNoBlock') }}</p>
        </div>
      </template>
  </Card>
</template>
