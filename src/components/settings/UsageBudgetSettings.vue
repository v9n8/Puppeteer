<script setup lang="ts">
// Settings → "Usage & Budget": context meter warning / limit override and the
// run-blocking usage budget.
import { useI18n } from 'vue-i18n'
import { Gauge } from 'lucide-vue-next'
import { Card, Input } from '@/components/ui'
import { useSettingsForm } from './useSettingsForm'

const { t } = useI18n()
const { settings } = useSettingsForm()
</script>

<template>
  <Card body-class="p-4 space-y-5">
    <template #header>
      <Gauge class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.usage.title') }}</span>
    </template>

      <!-- Context window meter -->
      <div class="space-y-3">
        <div>
          <h3 class="text-xs font-semibold">{{ t('settings.usage.contextMeter') }}</h3>
          <p class="text-2xs text-muted-foreground leading-relaxed mt-0.5">
            {{ t('settings.usage.contextMeterHint') }}
          </p>
        </div>
        <div class="grid grid-cols-2 gap-3">
          <div class="space-y-1.5">
            <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.usage.warnAt') }}</label>
            <Input v-model="settings.context_warn_percent" type="number" min="1" max="100" placeholder="80" />
          </div>
          <div class="space-y-1.5">
            <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.usage.limitOverride') }}</label>
            <Input v-model="settings.context_limit_override" type="number" min="0" :placeholder="t('settings.usage.limitOverridePlaceholder')" />
          </div>
        </div>
        <p class="text-2xs text-muted-foreground leading-relaxed">
          {{ t('settings.usage.limitOverrideHint') }}
        </p>
      </div>

      <hr class="border-border/60" />

      <!-- Usage budget: run-blocking guardrail -->
      <div class="space-y-3">
        <div>
          <h3 class="text-xs font-semibold">{{ t('settings.usage.budgetTitle') }}</h3>
          <p class="text-2xs text-muted-foreground leading-relaxed mt-0.5">
            {{ t('settings.usage.budgetHint') }}
          </p>
        </div>
        <div class="grid grid-cols-2 gap-3">
          <div class="space-y-1.5">
            <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.usage.block5h') }}</label>
            <Input v-model="settings.budget_5h_percent" type="number" min="1" max="100" :placeholder="t('settings.usage.emptyOff')" />
          </div>
          <div class="space-y-1.5">
            <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.usage.blockWeekly') }}</label>
            <Input v-model="settings.budget_week_percent" type="number" min="1" max="100" :placeholder="t('settings.usage.emptyOff')" />
          </div>
        </div>
      </div>
  </Card>
</template>
