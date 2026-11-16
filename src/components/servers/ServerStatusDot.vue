<script setup lang="ts">
// The status dot of a server row and of the list legend (V3-52): green online,
// red offline, grey ring not checked, spinner while a test runs.
import { useI18n } from 'vue-i18n'
import { Loader2 } from 'lucide-vue-next'
import type { ServerDot } from '@/lib/serverRows'

defineProps<{ dot: ServerDot }>()

const { t } = useI18n()

const TONE_CLASS = {
  online: 'bg-success',
  offline: 'bg-destructive',
  unknown: 'border border-muted-foreground/60',
} as const
</script>

<template>
  <span
    class="flex h-3 w-3 shrink-0 items-center justify-center"
    role="img"
    :title="t(dot.labelKey)"
    :aria-label="t(dot.labelKey)"
  >
    <Loader2 v-if="dot.tone === 'testing'" class="h-3 w-3 animate-spin text-muted-foreground" :stroke-width="2" />
    <span v-else class="h-2 w-2 rounded-full" :class="TONE_CLASS[dot.tone]" />
  </span>
</template>
