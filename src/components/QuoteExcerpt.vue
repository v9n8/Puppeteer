<script setup lang="ts">
// A quoted passage in the one Quote style (DS-3, option B): a quote mark and
// muted italic text, so it never looks like the note beside it. Code from a file
// stays upright in monospace: italic code is hard to read. Used by the cards on a
// sent message, the composer's Quote rows and the Quote note popover; each passes
// its own text size through `class`.
import { computed } from 'vue'
import { Quote } from 'lucide-vue-next'

const props = defineProps<{
  text: string
  /** Code from a file: monospace. */
  mono?: boolean
  /** Cut to this many lines. */
  lines?: number
  /** One line, cut with an ellipsis. */
  truncate?: boolean
}>()

// Whole class names, so Tailwind finds them. Never with `block`: it overrides
// line-clamp's `display: -webkit-box` and the text is no longer cut.
const CLAMP: Record<number, string> = { 1: 'line-clamp-1', 2: 'line-clamp-2', 3: 'line-clamp-3', 4: 'line-clamp-4', 5: 'line-clamp-5' }

const fit = computed(() => {
  if (props.truncate) return 'truncate'
  const clamp = props.lines ? CLAMP[props.lines] : undefined
  return clamp ? `${clamp} whitespace-pre-wrap` : 'block whitespace-pre-wrap'
})
</script>

<template>
  <span class="flex min-w-0 gap-1.5 text-muted-foreground" :class="truncate ? 'items-center' : ''">
    <Quote class="h-3 w-3 shrink-0" :class="truncate ? '' : 'mt-1'" :stroke-width="2" aria-hidden="true" />
    <span class="min-w-0 [overflow-wrap:anywhere]" :class="[fit, mono ? 'font-mono' : 'italic']">{{ text }}</span>
  </span>
</template>
