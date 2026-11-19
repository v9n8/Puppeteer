<script setup lang="ts">
// One option of a Settings screen (UX-33): label (+ ⓘ with the explanation) on
// the left, the control on the right, so an option takes one line instead of
// two. `description` joins `hint` in that ⓘ (UR-34), so no paragraph sits under
// the label. Below 900px window width the row
// stacks: label above, control below. `stacked` keeps that layout at every width, for a control that needs the
// full width (a picker grid, a list).
import { computed } from 'vue'
import { InfoTip } from '@/components/ui'

const props = withDefaults(defineProps<{
  label: string
  /** Explanation shown in the ⓘ tooltip. */
  hint?: string
  /** Further explanation, shown in the same ⓘ as `hint`. */
  description?: string
  stacked?: boolean
  /** Id the Settings search scrolls to (lib/settingsSearchRows); rendered as `data-settings-id`. */
  searchId?: string
}>(), {
  hint: undefined,
  description: undefined,
  stacked: false,
  searchId: undefined,
})

const tip = computed(() => [props.hint, props.description].filter(Boolean).join('\n\n'))
</script>

<template>
  <div
    :data-settings-id="searchId"
    class="flex flex-col gap-1.5"
    :class="!stacked && 'min-[901px]:flex-row min-[901px]:items-center min-[901px]:justify-between min-[901px]:gap-6'"
  >
    <div class="min-w-0">
      <div class="flex items-center gap-1.5">
        <span class="text-xs font-medium text-foreground">{{ label }}</span>
        <InfoTip v-if="tip" :text="tip" />
      </div>
    </div>
    <div :class="stacked ? 'w-full' : 'w-full min-[901px]:w-72 min-[901px]:shrink-0'">
      <slot />
    </div>
  </div>
</template>
