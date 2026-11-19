<script setup lang="ts">
// One quote on a sent message (UX-29) in the Quote style (DS-3): its number, the
// excerpt, the note and, for a file, the path. A card whose source is still there
// leads to it. Hover only changes the border: in the dark theme `accent` is the
// bubble's own colour, so a hover fill did not show. A gone source is dashed.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowUpRight, FileSymlink, FileText, Unlink } from 'lucide-vue-next'
import type { QuoteCard } from '@/lib/quote'
import QuoteExcerpt from './QuoteExcerpt.vue'

const props = defineProps<{ card: QuoteCard; index: number }>()
const emit = defineEmits<{ open: [] }>()

const { t } = useI18n()

const isFile = computed(() => props.card.quote.kind === 'file')
const gone = computed(() => props.card.quote.kind === 'entry' && !props.card.jumpable)
const title = computed(() =>
  props.card.jumpable ? (isFile.value ? t('streamLog.quotes.openFile') : t('streamLog.quotes.goToSource')) : undefined,
)
const sourceLabel = computed(() => {
  const q = props.card.quote
  if (q.kind !== 'file' || !q.path) return ''
  if (q.line_start == null) return q.path
  return q.line_end != null && q.line_end > q.line_start ? `${q.path}:${q.line_start}-${q.line_end}` : `${q.path}:${q.line_start}`
})
</script>

<template>
  <button
    type="button"
    class="group flex w-full items-start gap-2 rounded-md border border-border bg-card px-2.5 py-2 text-left transition-colors"
    :class="[
      card.jumpable ? 'cursor-pointer hover:border-primary/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring' : 'cursor-default',
      gone ? 'border-dashed' : '',
    ]"
    :disabled="!card.jumpable"
    :title="title"
    @click="emit('open')"
  >
    <span class="mt-0.5 flex h-5 min-w-5 shrink-0 items-center justify-center rounded-[4px] bg-muted px-1 text-2xs font-semibold text-muted-foreground">{{ index + 1 }}</span>
    <span class="min-w-0 flex-1">
      <QuoteExcerpt
        :text="card.quote.excerpt"
        :mono="isFile"
        :lines="3"
        :class="isFile ? 'text-xs leading-[1.5]' : 'text-md leading-[1.6]'"
      />
      <span
        v-if="card.quote.note"
        class="mt-1 block whitespace-pre-wrap pl-2.5 text-md leading-[1.6] text-foreground [overflow-wrap:anywhere]"
      >{{ card.quote.note }}</span>
      <span v-if="isFile" class="mt-1 flex items-center gap-1 text-2xs text-muted-foreground">
        <FileText class="h-3.5 w-3.5 shrink-0" :stroke-width="2" />
        <span class="truncate font-mono">{{ sourceLabel }}</span>
      </span>
      <span v-else-if="gone" class="mt-1 flex items-center gap-1 text-2xs italic text-muted-foreground">
        <Unlink class="h-3.5 w-3.5 shrink-0" :stroke-width="2" />
        {{ t('streamLog.quotes.sourceGone') }}
      </span>
    </span>
    <component
      :is="isFile ? FileSymlink : ArrowUpRight"
      v-if="card.jumpable"
      class="mt-1 h-3.5 w-3.5 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:text-primary group-hover:opacity-100 group-focus-visible:text-primary group-focus-visible:opacity-100"
      :stroke-width="2"
      aria-hidden="true"
    />
  </button>
</template>
