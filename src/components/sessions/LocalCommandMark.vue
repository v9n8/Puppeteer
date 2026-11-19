<script setup lang="ts">
// A local slash command the user ran (`/compact`, `/cost`…): a slim centred
// marker with its output under it (UR-16). Long output folds to 3 lines and
// breaks anywhere, so it never makes the log scroll sideways; a click shows it all.
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Terminal } from 'lucide-vue-next'

defineProps<{ name: string; args?: string; stdout?: string }>()
const { t } = useI18n()

const expanded = ref(false)
</script>

<template>
  <div class="my-1.5 flex min-w-0 flex-col items-center gap-1">
    <div
      v-if="name"
      class="inline-flex max-w-full items-center gap-1.5 rounded-full bg-muted/60 border border-border px-2.5 py-1 text-2xs"
    >
      <Terminal class="h-3 w-3 shrink-0 text-foreground/50" :stroke-width="1.75" />
      <span class="shrink-0 font-mono font-medium text-foreground/75">{{ name }}</span>
      <span v-if="args" class="min-w-0 truncate font-mono text-foreground/55" :title="args">{{ args }}</span>
    </div>
    <!-- role=button, not <button>: WebKit won't line-clamp a button. -->
    <div
      v-if="stdout"
      role="button"
      tabindex="0"
      class="max-w-full cursor-pointer whitespace-pre-wrap break-all text-2xs font-mono text-foreground/45 hover:text-foreground/70 transition-colors"
      :class="{ 'line-clamp-3': !expanded }"
      :title="expanded ? t('streamLog.hideOutput') : t('streamLog.showOutput')"
      :aria-expanded="expanded"
      @click="expanded = !expanded"
      @keydown.enter.prevent="expanded = !expanded"
    >{{ stdout }}</div>
  </div>
</template>
