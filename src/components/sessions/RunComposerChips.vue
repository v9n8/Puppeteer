<script setup lang="ts">
// The composer toolbar's always-visible chips (UX-26): permission mode, effort
// (only when the next turn's engine + model take one) and response style. State
// and defaults: useComposerChips.
import { Gauge, MessageSquareText } from 'lucide-vue-next'
import AppSelect from '@/components/AppSelect.vue'
import { PERMISSION_MODE_OPTIONS } from '@/lib/engineOptions'
import { useRunSession } from './useRunSession'

const {
  permissionMode, modeValue, modeChipLabel, modeChipIcon, modeChipTitle, effortPick, effortLevelsNow, effortOptions,
  effortChipLabel, effortChipTitle, stylePick, styleValue, styleOptions, styleChipLabel, styleChipTitle,
} = useRunSession()
</script>

<template>
  <AppSelect
    v-model="permissionMode"
    size="xs"
    variant="ghost"
    class="shrink-0"
    :options="PERMISSION_MODE_OPTIONS"
    :trigger-label="modeChipLabel"
    :aria-label="modeChipTitle"
    :title="modeChipTitle"
  >
    <template #leading>
      <component
        :is="modeChipIcon"
        class="h-3 w-3 shrink-0"
        :class="modeValue === 'bypassPermissions' ? 'text-destructive' : modeValue === 'plan' ? 'text-warning' : 'text-muted-foreground'"
        :stroke-width="1.75"
      />
    </template>
  </AppSelect>
  <AppSelect
    v-if="effortLevelsNow.length"
    v-model="effortPick"
    size="xs"
    variant="ghost"
    class="shrink-0"
    :options="effortOptions"
    :trigger-label="effortChipLabel"
    :aria-label="effortChipTitle"
    :title="effortChipTitle"
  >
    <template #leading>
      <Gauge class="h-3 w-3 shrink-0 text-muted-foreground" :stroke-width="1.75" />
    </template>
  </AppSelect>
  <AppSelect
    :model-value="styleValue"
    size="xs"
    variant="ghost"
    class="shrink-0 max-w-[11rem]"
    :options="styleOptions"
    :trigger-label="styleChipLabel"
    :aria-label="styleChipTitle"
    :title="styleChipTitle"
    @update:model-value="stylePick = $event"
  >
    <template #leading>
      <MessageSquareText class="h-3 w-3 shrink-0 text-muted-foreground" :stroke-width="1.75" />
    </template>
  </AppSelect>
</template>
