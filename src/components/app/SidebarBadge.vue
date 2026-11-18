<script lang="ts">
export interface NavBadge {
  count: number
  tone: 'danger' | 'warning'
  title?: string
}
</script>

<script setup lang="ts">
import CountPill from '@/components/ui/CountPill.vue'
// The count pill at the end of a sidebar row (overdue todos, sync conflicts).
defineProps<{ badge?: NavBadge | null }>()
</script>

<template>
  <CountPill v-if="badge && badge.count > 0" :tone="badge.tone" :title="badge.title">{{ badge.count }}</CountPill>
</template>
