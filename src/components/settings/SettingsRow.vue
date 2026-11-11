<script setup lang="ts">
// One option of a Settings screen (UX-33): label (+ ⓘ with the explanation) on
// the left, the control on the right, so an option takes one line instead of
// two. Below 900px window width the row stacks: label above, control below.
// `stacked` keeps that layout at every width, for a control that needs the
// full width (a picker grid, a list).
import { InfoTip } from '@/components/ui'

withDefaults(defineProps<{
  label: string
  /** Explanation shown in the ⓘ tooltip. */
  hint?: string
  stacked?: boolean
}>(), {
  hint: undefined,
  stacked: false,
})
</script>

<template>
  <div
    class="flex flex-col gap-1.5"
    :class="!stacked && 'min-[900px]:flex-row min-[900px]:items-center min-[900px]:justify-between min-[900px]:gap-6'"
  >
    <div class="flex min-w-0 items-center gap-1.5">
      <span class="text-xs font-medium text-foreground">{{ label }}</span>
      <InfoTip v-if="hint" :text="hint" />
    </div>
    <div :class="stacked ? 'w-full' : 'w-full min-[900px]:w-72 min-[900px]:shrink-0'">
      <slot />
    </div>
  </div>
</template>
