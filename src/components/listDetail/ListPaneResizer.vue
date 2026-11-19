<script setup lang="ts">
// Drag handle on the right edge of a list pane (UR-32). Put it inside a
// `relative` pane. Drag with the pointer, ← → to step, Home / End for the
// limits, double-click to reset. While dragging only `drag` fires; `commit`
// fires once on release or key, which is when the width is stored.
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { LIST_PANE_MAX, LIST_PANE_MIN, LIST_PANE_STEP } from '@/lib/listPaneWidth'

const props = defineProps<{ width: number }>()
const emit = defineEmits<{
  drag: [px: number]
  commit: [px: number]
  reset: []
}>()

const { t } = useI18n()
const dragging = ref(false)
let startX = 0
let startWidth = 0

function onDown(e: PointerEvent) {
  if (e.button !== 0) return
  e.preventDefault()
  dragging.value = true
  startX = e.clientX
  startWidth = props.width
  ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
}

function onMove(e: PointerEvent) {
  if (dragging.value) emit('drag', startWidth + e.clientX - startX)
}

function onUp(e: PointerEvent) {
  if (!dragging.value) return
  dragging.value = false
  emit('commit', startWidth + e.clientX - startX)
}

function onKey(e: KeyboardEvent) {
  let next: number | null = null
  if (e.key === 'ArrowLeft') next = props.width - LIST_PANE_STEP
  else if (e.key === 'ArrowRight') next = props.width + LIST_PANE_STEP
  else if (e.key === 'Home') next = LIST_PANE_MIN
  else if (e.key === 'End') next = LIST_PANE_MAX
  if (next === null) return
  e.preventDefault()
  emit('commit', next)
}
</script>

<template>
  <div
    role="separator"
    aria-orientation="vertical"
    tabindex="0"
    :aria-label="t('common.resizeList')"
    :aria-valuenow="width"
    :aria-valuemin="LIST_PANE_MIN"
    :aria-valuemax="LIST_PANE_MAX"
    :title="t('common.resizeList')"
    class="group absolute inset-y-0 -right-1.5 z-20 flex w-3 cursor-col-resize touch-none justify-center focus-visible:outline-none"
    @pointerdown="onDown"
    @pointermove="onMove"
    @pointerup="onUp"
    @pointercancel="onUp"
    @dblclick="emit('reset')"
    @keydown="onKey"
  >
    <span
      class="h-full transition-colors group-hover:w-0.5 group-hover:bg-primary group-focus-visible:w-0.5 group-focus-visible:bg-primary"
      :class="dragging ? 'w-0.5 bg-primary' : 'w-px'"
    />
  </div>
</template>
