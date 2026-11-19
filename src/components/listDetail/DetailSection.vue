<script setup lang="ts">
// A flat titled section of a list-detail screen's form (replaces stacked
// Cards): small upper-case heading, optional hint (an ⓘ by the heading, or a
// line under it with `hintVisible` for a warning that must not hide), then the fields.
import { InfoTip } from '@/components/ui'

defineProps<{
  title: string
  hint?: string
  hintVisible?: boolean
}>()

defineSlots<{
  default: () => unknown
  /** Right side of the heading row (e.g. a small action). */
  actions?: () => unknown
}>()
</script>

<template>
  <section class="space-y-3 border-b border-border py-4 last:border-b-0">
    <div class="flex items-center gap-2">
      <h2 class="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">{{ title }}</h2>
      <InfoTip v-if="hint && !hintVisible" :text="hint" />
      <span class="flex-1" />
      <slot name="actions" />
    </div>
    <p v-if="hint && hintVisible" class="-mt-1 text-2xs text-muted-foreground">{{ hint }}</p>
    <slot />
  </section>
</template>
