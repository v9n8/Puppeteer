<script setup lang="ts">
// The one empty state: icon tile, title, hint, then any actions (default slot).
// `fill` centres it in a whole page (Trash, Running, PR inbox); without it the
// block keeps only its padding (list panes, the schedule list, detail panes).
import type { Component } from 'vue'

withDefaults(defineProps<{ icon: Component; title?: string; hint?: string; fill?: boolean }>(), {
  title: undefined,
  hint: undefined,
  fill: false,
})
</script>

<template>
  <div
    class="flex flex-col items-center justify-center gap-1.5 px-4 py-8 text-center"
    :class="fill && 'h-full min-h-80'"
  >
    <div class="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl bg-muted text-muted-foreground">
      <component :is="icon" class="h-6 w-6" :stroke-width="1.5" />
    </div>
    <p v-if="title" class="text-sm font-semibold text-foreground">{{ title }}</p>
    <p v-if="hint" class="max-w-[46ch] text-xs text-muted-foreground">{{ hint }}</p>
    <slot />
  </div>
</template>
