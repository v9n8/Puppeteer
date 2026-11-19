<script setup lang="ts">
// The usage donut (UX-23), drawn by the status bar's usage block (DS-1): the
// collapsed button and its warning chips. `percent` fills the ring, coloured by
// `severity` (lib/usageRing).
import { computed } from 'vue'
import type { UsageSeverity } from '@/lib/usageRing'

const props = withDefaults(defineProps<{
  percent: number
  severity: UsageSeverity
  /** Size classes: 14px on the button, 12px on a chip. */
  size?: string
}>(), { size: 'h-3.5 w-3.5' })

const SEV_STROKE: Record<UsageSeverity, string> = { ok: 'stroke-info', warn: 'stroke-warning', over: 'stroke-destructive' }
const pct = computed(() => Math.min(100, Math.max(0, props.percent)))
</script>

<template>
  <svg class="shrink-0" :class="size" viewBox="0 0 36 36" aria-hidden="true">
    <circle class="fill-none stroke-muted" stroke-width="5" cx="18" cy="18" r="15.9155" />
    <circle
      v-if="pct > 0"
      class="fill-none transition-[stroke-dasharray] duration-300 motion-reduce:transition-none"
      :class="SEV_STROKE[severity]"
      stroke-width="5"
      stroke-linecap="round"
      cx="18"
      cy="18"
      r="15.9155"
      :style="{ strokeDasharray: `${pct} 100` }"
      transform="rotate(-90 18 18)"
    />
  </svg>
</template>
