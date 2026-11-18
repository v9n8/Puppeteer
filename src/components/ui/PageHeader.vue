<script setup lang="ts">
import CountPill from './CountPill.vue'
// Top bar of a full-page view (Projects, Stats, Focus, PR Inbox, …): fixed
// h-12 strip with the page title, an optional count pill, free-form content
// after the title (slot `meta`) and right-aligned actions (slot `actions`).
// List-detail screens use `listDetail/DetailHeader` for their right pane instead.
withDefaults(defineProps<{
  title: string
  /** Count pill after the title; hidden when null/undefined. */
  count?: number | string | null
  countTitle?: string
  /** Let a long title (project / run name) shrink with an ellipsis. */
  truncate?: boolean
}>(), {
  count: null,
  countTitle: undefined,
  truncate: false,
})

defineSlots<{
  /** Before the title (back button, page icon). */
  leading?: () => unknown
  meta?: () => unknown
  actions?: () => unknown
}>()
</script>

<template>
  <div class="flex h-12 shrink-0 items-center justify-between gap-2.5 border-b border-border/60 px-3">
    <div class="flex min-w-0 items-center gap-2.5">
      <slot name="leading" />
      <h1 class="text-[length:calc(0.9375rem*var(--text-scale))] font-semibold leading-[calc(1.3125rem*var(--text-scale))] text-muted-foreground" :class="truncate ? 'min-w-0 truncate' : 'shrink-0'" :title="truncate ? title : undefined">
        {{ title }}
      </h1>
      <CountPill v-if="count !== null && count !== undefined" :title="countTitle">{{ count }}</CountPill>
      <slot name="meta" />
    </div>
    <div v-if="$slots.actions" class="flex shrink-0 items-center gap-2">
      <slot name="actions" />
    </div>
  </div>
</template>
