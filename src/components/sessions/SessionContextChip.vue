<script setup lang="ts">
// Context chip of the session info bar (V3-16): a small bar and "82k/200k" (the
// number goes below 900px wide). The popover shows the same bar with its warning
// threshold and "Compact context", which sends /compact. The bar turns warning
// colour from the warn percent and destructive once the window is full.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { FoldVertical } from 'lucide-vue-next'
import { DropdownMenu, DropdownItem } from '@/components/ui'
import { formatTokensShort, resolveContextLimit } from '@/lib/contextLimits'
import { contextLevel } from '@/lib/sessionInfoBar'
import { useAppSettingsStore } from '@/stores/appSettings'
import { INFO_CHIP } from './infoBarStyle'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const appSettings = useAppSettingsStore()
const { contextTokens, contextModel, sendSlashCommand, sendingFollowUp } = useRunSession()

const warnPercent = computed(() => {
  const v = Number(appSettings.settings?.context_warn_percent)
  return Number.isFinite(v) && v > 0 ? v : 80
})
const limit = computed(() => {
  const override = Number(appSettings.settings?.context_limit_override)
  return resolveContextLimit(contextModel.value, Number.isFinite(override) ? override : null)
})
const rawPercent = computed(() => (limit.value > 0 ? (contextTokens.value / limit.value) * 100 : 0))
const percent = computed(() => Math.min(100, Math.round(rawPercent.value)))
// From the raw ratio: 99.6% rounds to 100 but the window is not full yet.
const level = computed(() => contextLevel(rawPercent.value, warnPercent.value))
const barClass = computed(() =>
  level.value === 'over' ? 'bg-destructive' : level.value === 'warn' ? 'bg-warning' : 'bg-primary/70',
)
const textClass = computed(() =>
  level.value === 'over' ? 'text-destructive' : level.value === 'warn' ? 'text-warning' : '',
)
const labels = computed(() => ({
  used: formatTokensShort(contextTokens.value),
  limit: formatTokensShort(limit.value),
  pct: percent.value,
}))
</script>

<template>
  <DropdownMenu v-if="contextTokens > 0" align="right" prefer="above">
    <template #trigger="{ open }">
      <button
        type="button"
        :class="INFO_CHIP"
        :title="t('run.infoBar.context.title', labels)"
        aria-haspopup="menu"
        :aria-expanded="open"
      >
        <span class="relative h-1 w-12 shrink-0 overflow-hidden rounded-full bg-muted">
          <span class="absolute inset-y-0 left-0 rounded-full transition-all" :class="barClass" :style="{ width: percent + '%' }" />
        </span>
        <span class="font-mono tabular-nums max-[900px]:hidden" :class="textClass">{{ labels.used }}/{{ labels.limit }}</span>
      </button>
    </template>
    <div class="w-[19rem]">
      <div class="flex flex-col gap-2 p-3" @click.stop>
        <p class="m-0 text-sm font-semibold">{{ t('run.infoBar.context.heading', labels) }}</p>
        <span class="relative block h-1.5 w-full overflow-hidden rounded-full bg-muted">
          <span class="absolute inset-y-0 left-0 rounded-full" :class="barClass" :style="{ width: percent + '%' }" />
        </span>
        <p class="m-0 text-2xs text-muted-foreground">{{ t('run.infoBar.context.warnLine', { pct: warnPercent }) }}</p>
      </div>
      <div class="border-t border-border pt-1">
        <DropdownItem :disabled="sendingFollowUp" @click="sendSlashCommand('/compact')">
          <FoldVertical class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
          <span class="min-w-0 flex-1">
            <span class="block">{{ t('run.statusBar.compact') }}</span>
            <span class="block whitespace-normal text-2xs text-muted-foreground">{{ t('run.infoBar.context.compactHint') }}</span>
          </span>
        </DropdownItem>
      </div>
    </div>
  </DropdownMenu>
</template>
