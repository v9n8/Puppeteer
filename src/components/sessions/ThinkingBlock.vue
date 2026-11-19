<script setup lang="ts">
// One thinking block of a turn's work (UR-16). Folded, it shows "Thinking" and
// its first line; a click opens just this block. It stays open while it is
// streamed and folds itself once done; a ⌘F hit in its text opens it.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Brain, ChevronRight } from 'lucide-vue-next'
import { thinkingOpen, thinkingPreview } from '@/lib/thinkingFold'

const props = defineProps<{ text: string; streaming: boolean; findHit: boolean }>()
const { t } = useI18n()

const choice = ref<boolean | undefined>(undefined)
// A new search hit beats an earlier "close", as for the turn's block.
watch(() => props.findHit, (hit) => { if (hit) choice.value = undefined })

const open = computed(() =>
  thinkingOpen({ choice: choice.value, streaming: props.streaming, findHit: props.findHit }),
)
const preview = computed(() => (open.value ? '' : thinkingPreview(props.text)))

function toggle() {
  choice.value = !open.value
}
</script>

<template>
  <div class="text-xs italic text-foreground/40 border-l-2 border-foreground/10 pl-3 py-1">
    <button
      type="button"
      class="flex w-full min-w-0 items-center gap-1 text-left cursor-pointer hover:text-foreground/70 transition-colors"
      :aria-expanded="open"
      :title="open ? t('streamLog.hideThinking') : t('streamLog.showThinking')"
      @click="toggle"
    >
      <ChevronRight class="h-3 w-3 shrink-0 transition-transform" :class="{ 'rotate-90': open }" :stroke-width="1.75" />
      <Brain class="h-3 w-3 shrink-0" :stroke-width="1.5" />
      <span class="shrink-0 text-2xs uppercase tracking-wider not-italic">{{ t('streamLog.thinking') }}</span>
      <span v-if="preview" class="min-w-0 truncate">{{ preview }}</span>
    </button>
    <div v-if="open" class="mt-1 min-w-0 whitespace-pre-wrap [overflow-wrap:anywhere]">{{ text }}</div>
  </div>
</template>
