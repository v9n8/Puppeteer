<script setup lang="ts">
// The Quote card above the composer (V3-33): the quotes that go ahead of the
// message, one row each, in a list that scrolls inside the card. The card folds,
// "Remove all" clears it, and its bottom handle sizes the list on its own,
// apart from the composer's handle. A dragged height and the folded state are
// kept only until the message is sent, the quotes are gone or the session
// changes; this component is mounted only while quotes exist, so it starts
// fresh each time the list goes empty.
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronDown, ChevronRight, Pencil, Quote, X } from 'lucide-vue-next'
import type { DraftQuote } from '@/lib/quote'
import { clampQuoteListHeight, defaultQuoteListHeight, stepQuoteListHeight } from '@/lib/quoteBlockHeight'

const props = defineProps<{
  quotes: DraftQuote[]
  runId: string | null
  /** True while the message is being sent: remove and note edits are locked. */
  sending: boolean
}>()
const emit = defineEmits<{
  remove: [id: string]
  setNote: [id: string, note: string]
}>()

const { t } = useI18n()

const ROW_PX = 32

const rootEl = ref<HTMLElement | null>(null)
const listEl = ref<HTMLElement | null>(null)
const userHeight = ref<number | null>(null)
const collapsed = ref(false)
const conversationPx = ref(480)
const dragging = ref(false)

const withNotes = computed(() => props.quotes.filter((q) => q.note?.trim()).length)
const multiple = computed(() => props.quotes.length > 1)
const defaultHeight = computed(() => defaultQuoteListHeight(ROW_PX, conversationPx.value))
const listHeight = computed(() =>
  userHeight.value == null ? defaultHeight.value : clampQuoteListHeight(userHeight.value, ROW_PX, conversationPx.value),
)

function fileLabel(q: DraftQuote): string {
  if (q.kind !== 'file') return ''
  const l = q.lines
  if (!l) return q.path
  return l.end > l.start ? `${q.path}:${l.start}-${l.end}` : `${q.path}:${l.start}`
}

// The conversation area is the Session frame (the grid that holds the
// conversation, files, status, composer) minus the composer; the list's own
// height is added back so growing the list does not shrink its own limit.
// The composer's parent is only its grid cell, so it cannot stand for the frame.
let observer: ResizeObserver | null = null
function measure() {
  const composer = rootEl.value?.closest<HTMLElement>('[data-run-composer]')
  const column = composer?.closest<HTMLElement>('[data-run-frame]')
  if (!composer || !column) return
  const own = listEl.value?.offsetHeight ?? 0
  conversationPx.value = Math.max(0, column.clientHeight - composer.offsetHeight + own)
}
onMounted(() => {
  measure()
  const composer = rootEl.value?.closest<HTMLElement>('[data-run-composer]')
  if (!composer || typeof ResizeObserver === 'undefined') return
  observer = new ResizeObserver(measure)
  observer.observe(composer)
  const frame = composer.closest<HTMLElement>('[data-run-frame]')
  if (frame) observer.observe(frame)
})
onBeforeUnmount(() => observer?.disconnect())

function reset() {
  userHeight.value = null
  collapsed.value = false
  editingId.value = null
}
watch(() => props.runId, reset)
watch(() => props.sending, (now, before) => { if (before && !now) reset() })

function removeAll() {
  for (const q of [...props.quotes]) emit('remove', q.id)
}

// Handle: drag, double-click to reset, arrow keys.
let dragStartY = 0
let dragStartHeight = 0
function onHandleDown(e: PointerEvent) {
  dragging.value = true
  dragStartY = e.clientY
  dragStartHeight = listHeight.value
  ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
}
function onHandleMove(e: PointerEvent) {
  if (!dragging.value) return
  userHeight.value = clampQuoteListHeight(dragStartHeight + e.clientY - dragStartY, ROW_PX, conversationPx.value)
}
function onHandleUp() {
  dragging.value = false
}
function onHandleKey(e: KeyboardEvent) {
  if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return
  e.preventDefault()
  userHeight.value = stepQuoteListHeight(listHeight.value, e.key === 'ArrowUp' ? 'up' : 'down', ROW_PX, conversationPx.value)
}

// In-place note editor. Enter saves, Shift+Enter breaks the line (a note typed in
// the Quote popover may span lines), Esc drops the change, leaving the field saves.
const editingId = ref<string | null>(null)
const editText = ref('')
const noteInput = ref<HTMLTextAreaElement[] | HTMLTextAreaElement | null>(null)
function startEdit(q: DraftQuote) {
  editingId.value = q.id
  editText.value = q.note ?? ''
  void nextTick(() => {
    const el = Array.isArray(noteInput.value) ? noteInput.value[0] : noteInput.value
    el?.focus()
  })
}
function saveEdit() {
  const id = editingId.value
  if (!id) return
  editingId.value = null
  const current = props.quotes.find((q) => q.id === id)
  if (current && (current.note ?? '') !== editText.value) emit('setNote', id, editText.value)
}
function cancelEdit() {
  editingId.value = null
}
function onNoteKey(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
    e.preventDefault()
    saveEdit()
  } else if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    cancelEdit()
  }
}

const btn = 'flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded text-foreground/60 hover:bg-background hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-40'
</script>

<template>
  <section ref="rootEl" class="mb-1.5 rounded-[10px] border border-border bg-muted text-xs">
    <header class="flex h-8 items-center gap-1 px-2">
      <component
        :is="multiple ? 'button' : 'div'"
        class="flex min-w-0 flex-1 items-center gap-1.5 rounded text-left"
        :class="multiple ? 'h-7 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring' : ''"
        v-bind="multiple ? {
          type: 'button',
          'aria-expanded': !collapsed,
          title: collapsed ? t('run.quotes.expand') : t('run.quotes.collapse'),
        } : {}"
        @click="multiple && (collapsed = !collapsed)"
      >
        <template v-if="multiple">
          <ChevronRight v-if="collapsed" class="h-3.5 w-3.5 shrink-0" :stroke-width="2" />
          <ChevronDown v-else class="h-3.5 w-3.5 shrink-0" :stroke-width="2" />
        </template>
        <Quote v-else class="h-3.5 w-3.5 shrink-0 opacity-60" :stroke-width="2" />
        <span class="truncate font-medium text-foreground">
          {{ t('run.quotes.count', { n: quotes.length }, quotes.length) }}<template v-if="withNotes">{{ ' ' }}{{ t('run.quotes.withNotes', { n: withNotes }) }}</template>
        </span>
      </component>
      <button
        v-if="multiple"
        type="button"
        class="h-7 shrink-0 cursor-pointer rounded px-2 text-foreground/70 hover:bg-background hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-default disabled:opacity-40"
        :title="t('run.quotes.removeAllTitle', { n: quotes.length })"
        :disabled="sending"
        @click="removeAll"
      >
        {{ t('run.quotes.removeAll') }}
      </button>
    </header>

    <template v-if="!collapsed">
      <ol
        ref="listEl"
        class="overflow-y-auto border-t border-border/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
        :style="multiple ? { maxHeight: `${listHeight}px`, height: `${listHeight}px` } : undefined"
        tabindex="0"
        :aria-label="t('run.quotes.listAria')"
      >
        <li v-for="(q, qi) in quotes" :key="q.id" class="px-2">
          <div class="flex h-8 items-center gap-2">
            <span class="flex h-4 min-w-4 shrink-0 items-center justify-center rounded-full bg-primary/15 px-1 text-2xs font-semibold text-primary">{{ qi + 1 }}</span>
            <span class="min-w-0 flex-1 truncate font-mono text-foreground/80" :title="q.excerpt">{{ q.excerpt }}</span>
            <span v-if="q.kind === 'file'" class="max-w-[30%] shrink-0 truncate font-mono text-2xs text-muted-foreground" :title="fileLabel(q)">{{ fileLabel(q) }}</span>
            <span
              v-if="q.note?.trim()"
              class="flex max-w-[30%] shrink-0 items-center gap-1 text-2xs text-muted-foreground"
              :title="t('run.quotes.noteTitle', { note: q.note })"
            >
              <Pencil class="h-3 w-3 shrink-0" :stroke-width="2" />
              <span class="truncate">{{ q.note }}</span>
            </span>
            <button
              type="button"
              :class="btn"
              :title="q.note ? t('run.quotes.editNote') : t('run.quotes.addNote')"
              :aria-label="q.note ? t('run.quotes.editNote') : t('run.quotes.addNote')"
              :disabled="sending"
              @click="startEdit(q)"
            >
              <Pencil class="h-3.5 w-3.5" :stroke-width="2" />
            </button>
            <button
              type="button"
              :class="btn"
              :title="t('run.quotes.remove')"
              :aria-label="t('run.quotes.remove')"
              :disabled="sending"
              @click="emit('remove', q.id)"
            >
              <X class="h-3.5 w-3.5" :stroke-width="2.5" />
            </button>
          </div>
          <div v-if="editingId === q.id" class="flex items-center gap-2 pb-1.5 pl-6">
            <textarea
              ref="noteInput"
              v-model="editText"
              rows="1"
              class="min-w-0 flex-1 resize-none rounded border border-border bg-background px-2 py-1 text-2xs text-foreground placeholder:text-muted-foreground/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              :placeholder="t('run.quotes.notePlaceholder')"
              :aria-label="t('run.quotes.notePlaceholder')"
              @keydown="onNoteKey"
              @blur="saveEdit"
            />
            <span class="shrink-0 text-2xs text-muted-foreground">{{ t('run.quotes.noteKeys') }}</span>
          </div>
        </li>
      </ol>

      <div
        v-if="multiple"
        role="separator"
        aria-orientation="horizontal"
        tabindex="0"
        :aria-valuenow="listHeight"
        :title="t('run.quotes.resize')"
        :aria-label="t('run.quotes.resize')"
        class="group flex h-4 cursor-row-resize touch-none items-center justify-center rounded-b-[10px] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        :class="dragging ? 'bg-accent' : ''"
        @pointerdown="onHandleDown"
        @pointermove="onHandleMove"
        @pointerup="onHandleUp"
        @pointercancel="onHandleUp"
        @dblclick="userHeight = null"
        @keydown="onHandleKey"
      >
        <span class="h-1 w-9 rounded-full" :class="dragging ? 'bg-primary' : 'bg-border group-hover:bg-foreground/40'" />
      </div>
    </template>
  </section>
</template>
