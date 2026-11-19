<script setup lang="ts">
// A quoted passage in the one Quote style (DS-3): muted text behind a left bar,
// as a blockquote reads in the conversation, so it never looks like the note
// beside it. Used by the cards on a sent message, the composer's Quote rows and
// the Quote note popover; each passes its own text size through `class`.
import { computed } from 'vue'

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
  if (props.truncate) return 'block truncate'
  const clamp = props.lines ? CLAMP[props.lines] : undefined
  return clamp ? `${clamp} whitespace-pre-wrap` : 'block whitespace-pre-wrap'
})
</script>

<template>
  <span
    class="min-w-0 border-l-2 border-muted-foreground/50 pl-2 text-muted-foreground [overflow-wrap:anywhere]"
    :class="[fit, mono ? 'font-mono' : '']"
  >{{ text }}</span>
</template>
