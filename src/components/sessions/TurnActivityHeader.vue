<script setup lang="ts">
// The header of one turn's folded work: "N steps · M files edited · K errors ·
// 12.3s", or a live "working…" while the turn runs. Clicking it shows or hides
// the turn's tool calls, thinking and commentary. components/StreamLog.vue
// places it above the block's first entry; which entries fold is decided by
// lib/turnActivity.
import { useI18n } from 'vue-i18n'
import { ChevronRight, ListTree } from 'lucide-vue-next'
import { formatActivityDuration, type TurnActivity } from '@/lib/turnActivity'

defineProps<{
  block: TurnActivity
  open: boolean
}>()

defineEmits<{ (e: 'toggle'): void }>()

const { t } = useI18n()
</script>

<template>
  <button
    type="button"
    class="flex w-full items-center gap-2 rounded-md px-1.5 py-1 text-left text-xs text-foreground/55 select-none transition-colors hover:bg-foreground/[0.06] hover:text-foreground/80 cursor-pointer"
    :aria-expanded="open"
    :title="open ? t('streamLog.activity.collapse') : t('streamLog.activity.expand')"
    @click="$emit('toggle')"
  >
    <ChevronRight
      class="h-3.5 w-3.5 shrink-0 text-foreground/40 transition-transform"
      :class="{ 'rotate-90': open }"
      :stroke-width="2"
    />
    <ListTree class="h-3.5 w-3.5 shrink-0 text-foreground/40" :stroke-width="1.75" />
    <span class="shrink-0 whitespace-nowrap">{{ t('streamLog.activity.steps', block.steps) }}</span>
    <template v-if="block.filesEdited">
      <span class="text-foreground/25">·</span>
      <span class="shrink-0 whitespace-nowrap">{{ t('streamLog.activity.filesEdited', block.filesEdited) }}</span>
    </template>
    <template v-if="block.errors">
      <span class="text-foreground/25">·</span>
      <span class="shrink-0 whitespace-nowrap text-destructive/80">{{ t('streamLog.activity.errors', block.errors) }}</span>
    </template>
    <span v-if="block.running" class="ml-auto shrink-0 text-2xs text-warning/80 animate-pulse">
      {{ t('streamLog.activity.working') }}
    </span>
    <span v-else-if="block.durationMs" class="ml-auto shrink-0 text-2xs font-mono tabular-nums text-foreground/40">
      {{ formatActivityDuration(block.durationMs) }}
    </span>
  </button>
</template>
