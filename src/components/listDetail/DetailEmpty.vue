<script setup lang="ts">
// Right pane of a list-detail screen when nothing is selected.
import type { Component } from 'vue'
import { EmptyState } from '@/components/ui'

defineProps<{
  icon: Component
  text: string
}>()
</script>

<template>
  <div class="flex min-w-0 flex-1 items-center justify-center p-6">
    <EmptyState :icon="icon" :hint="text">
      <slot />
    </EmptyState>
  </div>
</template>
