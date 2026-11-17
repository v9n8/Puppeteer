<script setup lang="ts">
// Panel container used across settings/detail screens. Optional `#header` slot
// renders a 44px transparent header row with a solid divider; body content goes
// in the default slot.
import { computed, useAttrs, useSlots } from 'vue'
import { cn } from '@/lib/utils'

defineOptions({ inheritAttrs: false })

withDefaults(defineProps<{
  /** Extra classes for the body wrapper (e.g. 'p-4' or 'divide-y divide-border/50'). */
  bodyClass?: string
}>(), { bodyClass: '' })

const slots = useSlots()
const attrs = useAttrs()
const rootClass = computed(() =>
  cn('ui-card border border-border bg-card rounded-card overflow-hidden', attrs.class as string | undefined),
)
</script>

<template>
  <div v-bind="{ ...attrs, class: undefined }" :class="rootClass">
    <div v-if="slots.header" class="flex items-center gap-2 min-h-11 px-3.5 border-b border-border">
      <slot name="header" />
    </div>
    <div :class="bodyClass">
      <slot />
    </div>
  </div>
</template>
