<script setup lang="ts">
// Settings → AI › "Sessions": the context meter (warning / limit override) and
// the usage quota tiers — a warning (`quota_warn_percent`, toast + badge colour)
// and the blocks on new sessions / follow-ups (`budget_5h_percent`,
// `budget_week_percent`, enforced by `stats::enforce_budget`), plus the opt-in
// stop of running sessions at those thresholds (`quota_stop_running`,
// `runs::quota_stop`) — and the limits on a turn nobody watches
// (`unattended_max_turns`, `unattended_max_minutes`, `runs::unattended`).
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Gauge } from 'lucide-vue-next'
import { Card, InfoTip, Input, SegmentedControl, ToggleSwitch, type SegmentedOption } from '@/components/ui'
import { useSettingsForm } from './useSettingsForm'
import SettingsRow from './SettingsRow.vue'

const { t } = useI18n()
const { settings } = useSettingsForm()

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

// Sub-tabs keep this section one screen tall. The choice is not saved.
type Tab = 'context' | 'quota' | 'unattended'
const tab = ref<Tab>('context')
const tabOptions = computed<SegmentedOption<Tab>[]>(() => [
  { value: 'context', label: t('settings.sessions.contextTitle') },
  { value: 'quota', label: t('settings.sessions.quotaTitle') },
  { value: 'unattended', label: t('settings.sessions.unattendedTitle') },
])
const tabHint = computed(() => ({
  context: t('settings.sessions.contextHint'),
  quota: t('settings.sessions.quotaHint'),
  unattended: t('settings.sessions.unattendedHint'),
})[tab.value])
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Gauge class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.sessions.title') }}</span>
    </template>

      <div class="flex items-center gap-2">
        <SegmentedControl v-model="tab" :options="tabOptions" :stretch="false" />
        <InfoTip :text="tabHint" />
      </div>

      <!-- Context window meter -->
      <template v-if="tab === 'context'">
        <SettingsRow :label="t('settings.sessions.warnAt')">
          <Input v-model="settings.context_warn_percent" type="number" min="1" max="100" placeholder="80" />
        </SettingsRow>
        <SettingsRow :label="t('settings.sessions.limitOverride')" :hint="t('settings.sessions.limitOverrideHint')">
          <Input v-model="settings.context_limit_override" type="number" min="0" :placeholder="t('settings.sessions.limitOverridePlaceholder')" />
        </SettingsRow>
      </template>

      <!-- Limits on a turn nobody watches: a schedule's, or one Remote Control started -->
      <template v-else-if="tab === 'unattended'">
        <SettingsRow :label="t('settings.sessions.unattendedMaxTurns')" :hint="t('settings.sessions.unattendedMaxTurnsHint')">
          <Input v-model="settings.unattended_max_turns" type="number" min="0" placeholder="200" />
        </SettingsRow>
        <SettingsRow :label="t('settings.sessions.unattendedMaxMinutes')" :hint="t('settings.sessions.unattendedMaxMinutesHint')">
          <Input v-model="settings.unattended_max_minutes" type="number" min="0" placeholder="60" />
        </SettingsRow>
        <p class="text-2xs text-muted-foreground leading-relaxed">{{ t('settings.sessions.unattendedStop') }}</p>
      </template>

      <!-- Usage quota: a warning tier, then the blocks on new sessions / follow-ups -->
      <template v-else>
        <SettingsRow
          :label="t('settings.sessions.quotaWarnAt')"
          :hint="`${t('settings.sessions.quotaWarnHint')}\n\n${t('settings.sessions.orderHint')}`"
        >
          <Input v-model="settings.quota_warn_percent" type="number" min="1" max="100" :placeholder="t('settings.sessions.emptyOff')" />
        </SettingsRow>
        <SettingsRow :label="t('settings.sessions.block5h')" :hint="t('settings.sessions.blockHint')">
          <Input v-model="settings.budget_5h_percent" type="number" min="1" max="100" :placeholder="t('settings.sessions.emptyOff')" />
        </SettingsRow>
        <SettingsRow :label="t('settings.sessions.blockWeekly')" :hint="t('settings.sessions.blockHint')">
          <Input v-model="settings.budget_week_percent" type="number" min="1" max="100" :placeholder="t('settings.sessions.emptyOff')" />
        </SettingsRow>
        <p v-if="warnNotBelowBlock" class="text-2xs text-warning leading-relaxed">
          {{ t('settings.sessions.warnNotBelowBlock') }}
        </p>
        <div class="space-y-1.5">
          <SettingsRow :label="t('settings.sessions.stopRunning')" :hint="t('settings.sessions.stopRunningHint')">
            <div class="flex min-[900px]:justify-end">
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
