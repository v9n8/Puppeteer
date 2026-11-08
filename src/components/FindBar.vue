<script setup lang="ts">
// The floating ⌘F bar for a usePageFind area: query, match case, `n/N`,
// previous / next, close. Enter / Shift+Enter step, Esc closes. The host places
// it (and shows it only while `find.open`).
import { useI18n } from 'vue-i18n'
import { CaseSensitive, ChevronDown, ChevronUp, Search, X } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import type { PageFind } from '@/composables/usePageFind'

const props = defineProps<{
  find: PageFind
  placeholder?: string
}>()

const { t } = useI18n()
// Refs pulled out so the template unwraps them; `inputEl` is bound as the
// input's template ref, which is what `openFind` focuses.
const { query, caseSensitive, count, current, inputEl } = props.find
</script>

<template>
  <div
    class="flex items-center gap-1 rounded-md border border-border bg-card px-1.5 py-1 shadow-lg shadow-black/30"
  >
    <Search class="h-3.5 w-3.5 text-foreground/40 shrink-0" :stroke-width="1.75" />
    <input
      ref="inputEl"
      v-model="query"
      type="text"
      :placeholder="placeholder || t('find.placeholder')"
      class="w-44 bg-transparent text-xs text-foreground placeholder:text-foreground/40 outline-none"
      @keydown.enter.exact.prevent="find.step(1)"
      @keydown.shift.enter.prevent="find.step(-1)"
      @keydown.esc.prevent="find.closeFind()"
    />
    <span class="text-2xs tabular-nums text-foreground/50 select-none shrink-0 min-w-[3rem] text-right">
      {{ query ? `${current}/${count}` : '' }}
    </span>
    <Button
      variant="ghost"
      size="icon-xs"
      class="shrink-0"
      :class="{ 'bg-primary/15 text-primary': caseSensitive }"
      :title="t('find.matchCase')"
      :aria-label="t('find.matchCase')"
      :aria-pressed="caseSensitive"
      @click="caseSensitive = !caseSensitive"
    >
      <CaseSensitive class="h-3.5 w-3.5" :stroke-width="1.75" />
    </Button>
    <Button
      variant="ghost"
      size="icon-xs"
      class="shrink-0"
      :title="t('find.previousMatch')"
      :aria-label="t('find.previousMatch')"
      :disabled="!count"
      @click="find.step(-1)"
    >
      <ChevronUp class="h-3.5 w-3.5" :stroke-width="1.75" />
    </Button>
    <Button
      variant="ghost"
      size="icon-xs"
      class="shrink-0"
      :title="t('find.nextMatch')"
      :aria-label="t('find.nextMatch')"
      :disabled="!count"
      @click="find.step(1)"
    >
      <ChevronDown class="h-3.5 w-3.5" :stroke-width="1.75" />
    </Button>
    <Button
      variant="ghost"
      size="icon-xs"
      class="shrink-0"
      :title="t('find.close')"
      :aria-label="t('find.close')"
      @click="find.closeFind()"
    >
      <X class="h-3.5 w-3.5" :stroke-width="1.75" />
    </Button>
  </div>
</template>

<!-- Custom Highlight API styling must be global: ::highlight() pseudo-elements
     are keyed off the document-wide CSS.highlights registry, so a scoped style
     (with its data-attribute) would never match. -->
<style>
::highlight(page-find) {
  background-color: rgba(250, 204, 21, 0.35);
  color: inherit;
}
::highlight(page-find-current) {
  background-color: rgba(249, 115, 22, 0.75);
  color: #fff;
}
</style>
