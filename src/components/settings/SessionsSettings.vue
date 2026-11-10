<script setup lang="ts">
// Settings → AI › "Sessions": the context meter (warning / limit override) and
// the usage quota tiers — a warning (`quota_warn_percent`, toast + badge colour)
// and the blocks on new sessions / follow-ups (`budget_5h_percent`,
// `budget_week_percent`, enforced by `stats::enforce_budget`), plus the opt-in
// stop of running sessions at those thresholds (`quota_stop_running`,
// `runs::quota_stop`).
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Gauge } from 'lucide-vue-next'
import { Card, Input, ToggleSwitch } from '@/components/ui'
import { useSettingsForm } from './useSettingsForm'

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
</script>

<template>
  <Card body-class="p-4 space-y-5">
    <template #header>
      <Gauge class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.sessions.title') }}</span>
    </template>

      <!-- Context window meter -->
      <div class="space-y-3">
        <div>
          <h3 class="text-xs font-semibold">{{ t('settings.sessions.contextTitle') }}</h3>
          <p class="text-2xs text-muted-foreground leading-relaxed mt-0.5">
            {{ t('settings.sessions.contextHint') }}
          </p>
        </div>
        <div class="grid grid-cols-2 gap-3">
          <div class="space-y-1.5">
            <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.sessions.warnAt') }}</label>
            <Input v-model="settings.context_warn_percent" type="number" min="1" max="100" placeholder="80" />
          </div>
          <div class="space-y-1.5">
            <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.sessions.limitOverride') }}</label>
            <Input v-model="settings.context_limit_override" type="number" min="0" :placeholder="t('settings.sessions.limitOverridePlaceholder')" />
          </div>
        </div>
        <p class="text-2xs text-muted-foreground leading-relaxed">
          {{ t('settings.sessions.limitOverrideHint') }}
        </p>
      </div>

      <hr class="border-border/60" />

      <!-- Usage quota: a warning tier, then the blocks on new sessions / follow-ups -->
      <div class="space-y-3">
        <div>
          <h3 class="text-xs font-semibold">{{ t('settings.sessions.quotaTitle') }}</h3>
          <p class="text-2xs text-muted-foreground leading-relaxed mt-0.5">
            {{ t('settings.sessions.quotaHint') }}
          </p>
        </div>
        <div class="space-y-1.5">
          <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.sessions.quotaWarnAt') }}</label>
          <Input v-model="settings.quota_warn_percent" type="number" min="1" max="100" :placeholder="t('settings.sessions.emptyOff')" />
          <p class="text-2xs text-muted-foreground leading-relaxed">
            {{ t('settings.sessions.quotaWarnHint') }}
          </p>
        </div>
        <div class="grid grid-cols-2 gap-3">
          <div class="space-y-1.5">
            <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.sessions.block5h') }}</label>
            <Input v-model="settings.budget_5h_percent" type="number" min="1" max="100" :placeholder="t('settings.sessions.emptyOff')" />
          </div>
          <div class="space-y-1.5">
            <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.sessions.blockWeekly') }}</label>
            <Input v-model="settings.budget_week_percent" type="number" min="1" max="100" :placeholder="t('settings.sessions.emptyOff')" />
          </div>
        </div>
        <p class="text-2xs text-muted-foreground leading-relaxed">
          {{ t('settings.sessions.blockHint') }}
        </p>
        <p class="text-2xs leading-relaxed" :class="warnNotBelowBlock ? 'text-warning' : 'text-muted-foreground'">
          {{ warnNotBelowBlock ? t('settings.sessions.warnNotBelowBlock') : t('settings.sessions.orderHint') }}
        </p>
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0">
            <div class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.sessions.stopRunning') }}</div>
            <p class="text-2xs text-muted-foreground leading-relaxed mt-0.5">{{ t('settings.sessions.stopRunningHint') }}</p>
            <p class="text-2xs leading-relaxed mt-0.5" :class="stopRunning ? 'text-warning' : 'text-muted-foreground'">{{ t('settings.sessions.stopRunningWarning') }}</p>
            <p v-if="!hasBlock" class="text-2xs text-muted-foreground leading-relaxed mt-0.5">{{ t('settings.sessions.stopRunningNoBlock') }}</p>
          </div>
          <ToggleSwitch
            size="md"
            :checked="stopRunning"
            :disabled="!hasBlock && !stopRunning"
            :label="t('settings.sessions.stopRunning')"
            @toggle="settings.quota_stop_running = stopRunning ? 'false' : 'true'"
          />
        </div>
      </div>
  </Card>
</template>
