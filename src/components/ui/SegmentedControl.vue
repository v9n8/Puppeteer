<script setup lang="ts" generic="T extends string | number">
// Pill-style switcher (filters, view modes, auth method…): a bordered muted
// track (30px) with the active segment (24px) lifted on a card chip.
//
// - `stretch` (default) spreads segments over the full width (sidebar filters).
// - `radio` makes it a `radiogroup` of `radio`s (`aria-checked`) instead of a
//   `group` of toggle buttons; pass the group's `aria-label` as an attribute.
// - Labels never wrap: they truncate, and `title` (default `label`) shows the full text.
// - `iconOnly` renders just the icon; `label` becomes the tooltip/aria-label.
import type { Component } from 'vue'
import { cn } from '@/lib/utils'

export interface SegmentedOption<V> {
  value: V
  label: string
  icon?: Component
  /** Tooltip; defaults to `label` when `iconOnly`. */
  title?: string
  disabled?: boolean
}

const props = withDefaults(defineProps<{
  modelValue: T
  options: SegmentedOption<T>[]
  stretch?: boolean
  iconOnly?: boolean
  radio?: boolean
  class?: string
}>(), {
  stretch: true,
  iconOnly: false,
  radio: false,
  class: undefined,
})

const emit = defineEmits<{ 'update:modelValue': [value: T] }>()
</script>

<template>
  <div
    :class="cn('ui-segmented flex items-center gap-0.5 rounded-md border border-border bg-muted p-0.5 text-2xs font-normal text-muted-foreground', !stretch && 'shrink-0', props.class)"
    :role="radio ? 'radiogroup' : 'group'"
  >
    <button
      v-for="o in options"
      :key="String(o.value)"
      type="button"
      class="relative flex h-6 min-w-0 cursor-pointer items-center whitespace-nowrap justify-center rounded-sm transition-colors before:absolute before:-inset-y-0.5 before:inset-x-0 before:content-[''] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
      :class="[
        stretch && 'flex-1',
        iconOnly ? 'w-6 before:-inset-x-0.5' : 'px-2.5',
        o.icon && !iconOnly && 'gap-1',
        modelValue === o.value
          ? 'is-active bg-card font-medium text-foreground ring-1 ring-input'
          : 'text-muted-foreground hover:text-foreground',
      ]"
      :title="o.title ?? o.label"
      :aria-label="iconOnly ? o.label : undefined"
      :role="radio ? 'radio' : undefined"
      :aria-pressed="radio ? undefined : modelValue === o.value"
      :aria-checked="radio ? modelValue === o.value : undefined"
      :disabled="o.disabled"
      @click="emit('update:modelValue', o.value)"
    >
      <component :is="o.icon" v-if="o.icon" :class="iconOnly ? 'h-3.5 w-3.5' : 'h-3 w-3'" :stroke-width="iconOnly ? 1.75 : 2" />
      <span v-if="!iconOnly" class="min-w-0 truncate">{{ o.label }}</span>
    </button>
  </div>
</template>
