<script setup lang="ts">
// The strip at the top of the "Open windows" grid: always there so the cards read as
// draggable, it takes the accent while cards that can move back are dragged and turns
// amber (nothing happens on drop) when none of them can. useWindowsDrag finds it by
// `data-windows-drop`.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Ban, SquareArrowDownLeft } from 'lucide-vue-next'

const props = defineProps<{
  /** Windows dragged; 0 when nothing is. */
  dragged: number
  /** Of them, the ones that can move back. */
  mergeable: number
  /** Title of the card held under the pointer. */
  name: string
}>()

const { t } = useI18n()

const tone = computed(() => {
  if (props.dragged === 0) return 'idle'
  return props.mergeable === 0 ? 'refused' : 'merge'
})
const text = computed(() => {
  if (tone.value === 'idle') return t('windows.drop.idle')
  if (tone.value === 'refused') return t('windows.drop.refused', { name: props.name })
  return props.dragged === 1
    ? t('windows.drop.one', { name: props.name })
    : t('windows.drop.many', { count: props.mergeable }, props.mergeable)
})
</script>

<template>
  <!-- Opaque, and stuck over the grid's top padding too: the cards scrolling under it stay hidden. -->
  <div class="sticky -top-3 z-[2] -mt-3 mb-3 select-none bg-popover pt-3">
    <div
      data-windows-drop
      class="flex h-20 items-center justify-center gap-2 rounded-card border-2 border-dashed px-3 text-sm"
      :class="{
        idle: 'border-input text-muted-foreground',
        merge: 'border-primary bg-primary/10 text-primary',
        refused: 'cursor-not-allowed border-warning bg-warning/10 text-warning',
      }[tone]"
      role="status"
      aria-live="polite"
    >
      <component :is="tone === 'refused' ? Ban : SquareArrowDownLeft" class="h-4 w-4 shrink-0" :stroke-width="1.75" aria-hidden="true" />
      <span class="min-w-0 truncate" :class="tone === 'idle' ? '' : 'font-medium'" :title="text">{{ text }}</span>
    </div>
  </div>
</template>
