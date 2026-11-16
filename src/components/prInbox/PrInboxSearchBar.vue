<script setup lang="ts">
// Search row of the PR inbox: a wide box + "Search" button + "Clear filters",
// then the Filters / Display popovers in the `trailing` slot. What is typed
// (`draft`) stays apart from what the list uses (`applied`): only the button
// or Enter applies it (lib/prInboxSearch). The size container lets the
// popover buttons drop to icon-only when the content pane is narrow.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Button, Input } from '@/components/ui'
import { IS_MAC } from '@/lib/appMenu'
import type { PrFilters } from '@/lib/prInboxFilters'
import { applySearch, canClear } from '@/lib/prInboxSearch'
import { Search, X } from 'lucide-vue-next'

const props = defineProps<{
  /** The text the list is filtered by right now. */
  applied: string
  /** Filters that currently apply (decides whether "Clear filters" shows). */
  filters: PrFilters
}>()

const emit = defineEmits<{
  /** "Search" / Enter: filter the list by this text (empty = no text filter). */
  apply: [text: string]
  /** Esc / x in the box: drop the text and the text filter. */
  clear: []
  /** "Clear filters": drop every filter and the text. */
  clearAll: []
}>()

const { t } = useI18n()

const draft = ref(props.applied)
watch(() => props.applied, v => { draft.value = v })

const wrap = ref<HTMLElement | null>(null)
const inputEl = () => wrap.value?.querySelector('input') ?? null
const keyLabel = IS_MAC ? '⌘F' : 'Ctrl+F'
const clearable = computed(() => canClear({ draft: draft.value, applied: props.applied }, props.filters))

function submit() {
  emit('apply', applySearch({ draft: draft.value, applied: props.applied }).applied)
}

function clearText() {
  draft.value = ''
  emit('clear')
}

defineExpose({
  /** Put the cursor in the box with the current text selected (⌘F). */
  focus() {
    const el = inputEl()
    if (!el) return false
    el.focus()
    el.select()
    return true
  },
  /** Apply the box text now (used by the Filters popover's "Search"). */
  submit,
  /** Empty the box without touching what is applied. */
  reset() { draft.value = '' },
})
</script>

<template>
  <div class="@container/toolbar relative z-10 flex h-12 shrink-0 items-center border-b border-border/60 px-6">
    <div class="flex min-w-0 flex-1 items-center gap-2">
      <div ref="wrap" class="relative min-w-40 flex-1">
        <Search
          class="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
          :stroke-width="1.75"
        />
        <Input
          v-model="draft"
          class="pl-8 pr-12"
          :placeholder="t('prInbox.searchPlaceholder')"
          :aria-label="t('prInbox.searchPlaceholder')"
          @keydown.enter.prevent="submit"
          @keydown.esc.prevent="clearText"
        />
        <button
          v-if="draft"
          type="button"
          class="absolute right-2 top-1/2 -translate-y-1/2 cursor-pointer text-muted-foreground hover:text-foreground"
          :title="t('prInbox.clearSearch')"
          :aria-label="t('prInbox.clearSearch')"
          @click="clearText"
        >
          <X class="h-3.5 w-3.5" :stroke-width="2" />
        </button>
        <span
          v-else
          class="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-2xs text-muted-foreground"
          :title="t('prInbox.findHint', { key: keyLabel })"
        >
          {{ keyLabel }}
        </span>
      </div>
      <Button variant="outline" :title="t('prInbox.searchTitle')" @click="submit">
        <Search class="h-3.5 w-3.5" :stroke-width="1.75" />
        {{ t('prInbox.searchButton') }}
      </Button>
      <Button v-if="clearable" variant="ghost" @click="emit('clearAll')">
        {{ t('prInbox.filters.clearAll') }}
      </Button>
      <span class="h-5 w-px shrink-0 bg-border/60" aria-hidden="true" />
      <div class="flex shrink-0 items-center gap-2">
        <slot name="trailing" />
      </div>
    </div>
  </div>
</template>
