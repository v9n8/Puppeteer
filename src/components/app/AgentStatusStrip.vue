<script setup lang="ts">
// 2px solid strip under the title bar showing what the agents are doing. No
// animation or transition on purpose: colour changes apply instantly.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAgentAmbient } from '@/composables/useAgentAmbient'

const { t } = useI18n()
const { state } = useAgentAmbient()

const COLOR = {
  running: 'bg-info',
  waiting: 'bg-warning',
  error: 'bg-destructive',
  done: 'bg-success',
} as const

const color = computed(() => (state.value === 'idle' ? '' : COLOR[state.value]))
const label = computed(() => (state.value === 'idle' ? undefined : t(`misc.agentStrip.${state.value}`)))
</script>

<template>
  <div role="status" class="h-0.5 shrink-0" :class="color" :title="label" :aria-label="label" />
</template>
