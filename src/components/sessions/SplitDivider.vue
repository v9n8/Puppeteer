<script setup lang="ts">
// The bar between Conversation and the files in the split view (V3-27). Drag it
// with the pointer, double-click for 50 / 50, or focus it and use ← → (5%) and
// Home / End (the limits). While dragging only `drag` fires (the parent shows
// it live); `commit` fires once on release or key, which is when it is stored.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { SPLIT_MIN_SIDE, SPLIT_STEP, clampRatio, ratioLimits } from '@/lib/sessionTabs'

const props = defineProps<{
  /** Left share (0..1) as shown now. */
  ratio: number
  /** The grid the bar divides; its width is the 100%. */
  container: HTMLElement | null
}>()
const emit = defineEmits<{
  drag: [ratio: number]
  commit: [leftPx: number, totalPx: number]
  reset: []
}>()

const { t } = useI18n()
const dragging = ref(false)
const total = () => Math.max((props.container?.clientWidth ?? 0) - 1, 1)

const left = computed(() => Math.round(props.ratio * 100))
const limits = computed(() => {
  const { min, max } = ratioLimits(total())
  return { min: Math.round(min * 100), max: Math.round(max * 100) }
})

function onDown(e: PointerEvent) {
  if (e.button !== 0 || !props.container) return
  e.preventDefault()
  dragging.value = true
  ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
}

function ratioAt(clientX: number): number {
  const rect = props.container?.getBoundingClientRect()
  return clampRatio(clientX - (rect?.left ?? 0), total())
}

function onMove(e: PointerEvent) {
  if (dragging.value) emit('drag', ratioAt(e.clientX))
}

function onUp(e: PointerEvent) {
  if (!dragging.value) return
  dragging.value = false
  const r = ratioAt(e.clientX)
  emit('commit', r * total(), total())
}

function onKey(e: KeyboardEvent) {
  const { min, max } = ratioLimits(total())
  let next: number | null = null
  if (e.key === 'ArrowLeft') next = props.ratio - SPLIT_STEP
  else if (e.key === 'ArrowRight') next = props.ratio + SPLIT_STEP
  else if (e.key === 'Home') next = min
  else if (e.key === 'End') next = max
  if (next === null) return
  e.preventDefault()
  emit('commit', next * total(), total())
}
</script>

<template>
  <div class="relative h-full w-px bg-border">
    <div
      role="separator"
      aria-orientation="vertical"
      tabindex="0"
      :aria-label="t('run.splitView.dividerLabel')"
      :aria-valuenow="left"
      :aria-valuemin="limits.min"
      :aria-valuemax="limits.max"
      :aria-valuetext="t('run.splitView.dragTip', { l: left, r: 100 - left, px: SPLIT_MIN_SIDE })"
      :title="t('run.splitView.dividerTip')"
      class="group absolute inset-y-0 -left-3.5 z-20 flex w-7 cursor-col-resize touch-none items-center justify-center focus-visible:outline-none"
      @pointerdown="onDown"
      @pointermove="onMove"
      @pointerup="onUp"
      @pointercancel="onUp"
      @dblclick="emit('reset')"
      @keydown="onKey"
    >
      <span
        class="absolute inset-y-0 left-1/2 -translate-x-1/2 transition-colors group-hover:w-0.5 group-hover:bg-primary group-focus-visible:w-0.5 group-focus-visible:bg-primary"
        :class="dragging ? 'w-0.5 bg-primary' : 'w-px'"
      />
      <span
        class="relative h-8 w-[7px] rounded-full border border-border bg-card group-focus-visible:ring-2 group-focus-visible:ring-ring"
        :class="{ 'border-primary': dragging }"
        aria-hidden="true"
      />
      <span
        v-if="dragging"
        class="pointer-events-none absolute left-1/2 top-3 z-30 -translate-x-1/2 whitespace-nowrap rounded-md border border-border bg-popover px-2 py-1 text-2xs text-foreground shadow-lg shadow-black/20"
      >
        {{ t('run.splitView.dragTip', { l: left, r: 100 - left, px: SPLIT_MIN_SIDE }) }}
      </span>
    </div>
  </div>
</template>
