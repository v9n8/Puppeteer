<script lang="ts">
export interface NavBadge {
  count: number
  tone: 'danger' | 'warning'
  title?: string
}
</script>

<script setup lang="ts">
// The count pill at the end of a sidebar row (overdue todos, sync conflicts).
defineProps<{ badge?: NavBadge | null }>()
</script>

<template>
  <span
    v-if="badge && badge.count > 0"
    class="flex h-4 min-w-[16px] items-center justify-center rounded-full px-1 text-2xs font-medium leading-none"
    :class="badge.tone === 'danger' ? 'bg-destructive text-destructive-foreground' : 'bg-warning text-warning-foreground'"
    :title="badge.title"
  >
    {{ badge.count }}
  </span>
</template>
