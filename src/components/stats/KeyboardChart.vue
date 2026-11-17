<script setup lang="ts">
// A chart.js bar chart you can walk with the keyboard: Tab focuses the frame, ← → step
// through the days (Home / End jump to the ends), the chart.js tooltip follows the
// selected column exactly as when hovering, and a polite live region reads it out.
// Hover keeps working as before; leaving focus hides the tooltip.
import { computed, ref } from 'vue'
import type { Chart as ChartJS, ChartData, ChartOptions } from 'chart.js'
import { Bar } from 'vue-chartjs'
import { moveCursor } from '@/lib/listCursor'

const props = defineProps<{
  data: ChartData<'bar'>
  options: ChartOptions<'bar'>
  /** Spoken text for the day at `index`, e.g. "05-12: 1,200 token, $0.40". */
  describe: (index: number) => string
  label: string
}>()

const chartRef = ref<{ chart?: ChartJS } | null>(null)
const cursor = ref(-1)
const count = computed<number>(() => props.data?.labels?.length ?? 0)
const spoken = computed(() => (cursor.value >= 0 && cursor.value < count.value ? props.describe(cursor.value) : ''))

function show(index: number) {
  const chart = chartRef.value?.chart
  if (!chart) return
  const active = chart.data.datasets.map((_: unknown, datasetIndex: number) => ({ datasetIndex, index }))
  const anchor = chart.getDatasetMeta(0).data[index]
  chart.setActiveElements(active)
  chart.tooltip?.setActiveElements(active, { x: anchor?.x ?? 0, y: anchor?.y ?? 0 })
  chart.update('none')
}

function hide() {
  const chart = chartRef.value?.chart
  cursor.value = -1
  if (!chart) return
  chart.setActiveElements([])
  chart.tooltip?.setActiveElements([], { x: 0, y: 0 })
  chart.update('none')
}

function onKey(e: KeyboardEvent) {
  const n = count.value
  if (n === 0) return
  let next: number
  if (e.key === 'ArrowRight') next = moveCursor(cursor.value, n, 'ArrowDown')
  else if (e.key === 'ArrowLeft') next = cursor.value < 0 ? n - 1 : Math.max(0, cursor.value - 1)
  else if (e.key === 'Home') next = moveCursor(cursor.value, n, 'Home')
  else if (e.key === 'End') next = moveCursor(cursor.value, n, 'End')
  else return
  e.preventDefault()
  cursor.value = next
  show(next)
}
</script>

<template>
  <div
    class="h-full rounded-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    :tabindex="count > 0 ? 0 : -1"
    role="img"
    :aria-label="label"
    @keydown="onKey"
    @blur="hide"
  >
    <Bar ref="chartRef" :data="data" :options="options" />
    <span class="sr-only" aria-live="polite">{{ spoken }}</span>
  </div>
</template>
