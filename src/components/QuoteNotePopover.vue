<script setup lang="ts">
// The note asked for before a quote goes into the composer (UX-46): the
// excerpt, a note field, Close and Save. Opened by the selection toolbar's Quote
// and by the Quote under a turn; SelectionToolbar.vue renders it. Dragged by its
// header, resized from its bottom-right corner, always inside the window
// (useDragPosition). Each opening starts next to where Quote was clicked: where
// it was dragged last time is not remembered (as in AWOG).
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Quote } from 'lucide-vue-next'
import { Button, Textarea } from '@/components/ui'
import { useDragPosition } from '@/composables/useDragPosition'
import { toolbarPosition } from '@/lib/selectionSurface'

const props = defineProps<{
  text: string
  /** Name of the session the quote goes to. */
  target: string
  /** What Quote was clicked from: the popover opens under it, or above it near the bottom. */
  at: { left: number; top: number; bottom: number }
}>()

const emit = defineEmits<{ save: [note: string]; close: [] }>()

const { t } = useI18n()

const WIDTH = 280
const rootEl = ref<HTMLElement | null>(null)
const noteField = ref<InstanceType<typeof Textarea> | null>(null)
const note = ref('')
// Hidden until measured and placed, so it never flashes at the top-left corner.
const placed = ref(false)
const { pos, size, startMove, startResize } = useDragPosition(rootEl, { minWidth: 240, maxWidth: 560, minHeight: 160 })

const heading = computed(() => t('selection.quote', { target: props.target }))
const style = computed(() => ({
  left: `${pos.value.x}px`,
  top: `${pos.value.y}px`,
  width: `${size.value?.width ?? WIDTH}px`,
  ...(size.value ? { height: `${size.value.height}px` } : {}),
  visibility: placed.value ? undefined : ('hidden' as const),
}))

function save() {
  emit('save', note.value)
}

// Enter or ⌘Enter saves, Shift+Enter is a new line. Not while an IME is
// composing: its Enter picks the candidate.
function onNoteKeydown(e: KeyboardEvent) {
  if (e.key !== 'Enter' || e.shiftKey || e.isComposing || e.keyCode === 229) return
  e.preventDefault()
  save()
}

// Capture phase: Esc closes this popover, not the dialog it may sit over (MessageFullscreen).
function onKeydown(e: KeyboardEvent) {
  if (e.key !== 'Escape' || e.isComposing) return
  e.preventDefault()
  e.stopPropagation()
  emit('close')
}

onMounted(async () => {
  window.addEventListener('keydown', onKeydown, true)
  await nextTick()
  const el = rootEl.value
  if (!el) return
  pos.value = toolbarPosition(
    props.at,
    { width: el.offsetWidth, height: el.offsetHeight },
    { width: window.innerWidth, height: window.innerHeight },
  )
  placed.value = true
  ;(noteField.value?.$el as HTMLTextAreaElement | undefined)?.focus({ preventScroll: true })
})
onUnmounted(() => window.removeEventListener('keydown', onKeydown, true))
</script>

<template>
  <Teleport to="body">
    <!-- Takes the clicks outside: they do not close the popover, so a half-typed note is never lost (D7). -->
    <div class="fixed inset-0 z-[70]" @mousedown.prevent />
    <div
      ref="rootEl"
      role="dialog"
      aria-modal="true"
      :aria-label="heading"
      class="fixed z-[70] flex flex-col overflow-hidden rounded-xl border border-border bg-card shadow-xl shadow-black/30"
      :style="style"
    >
      <div
        class="shrink-0 cursor-move select-none border-b border-border bg-muted/40 px-3 py-2"
        :title="t('selection.quoteNote.move')"
        @pointerdown="startMove"
      >
        <div class="flex items-center gap-1.5 text-2xs font-semibold text-foreground/80">
          <Quote class="h-3 w-3 shrink-0 text-primary/80" :stroke-width="2" />
          <span class="truncate" :title="heading">{{ heading }}</span>
        </div>
        <p class="mt-1 max-h-24 overflow-y-auto whitespace-pre-wrap break-words text-xs text-foreground/70">{{ text }}</p>
      </div>
      <div class="flex min-h-0 flex-1 flex-col gap-2 p-2">
        <Textarea
          ref="noteField"
          v-model="note"
          rows="3"
          class="min-h-0 flex-1 resize-none"
          :placeholder="t('selection.quoteNote.notePlaceholder')"
          :aria-label="t('selection.quoteNote.notePlaceholder')"
          @keydown="onNoteKeydown"
        />
        <div class="flex shrink-0 items-center gap-1.5 pr-2">
          <span class="min-w-0 flex-1 truncate text-2xs text-muted-foreground" :title="t('selection.quoteNote.keys')">
            {{ t('selection.quoteNote.keys') }}
          </span>
          <Button variant="ghost" size="xs" @click="emit('close')">{{ t('common.close') }}</Button>
          <Button size="xs" @click="save">{{ t('common.save') }}</Button>
        </div>
      </div>
      <!-- Visible grip (UX-65): the note field fills the popover, so this corner sizes it. -->
      <div
        class="absolute bottom-0 right-0 flex h-4 w-4 cursor-nwse-resize items-end justify-end p-0.5 text-muted-foreground/60 hover:text-foreground"
        :title="t('selection.quoteNote.resize')"
        @pointerdown="startResize"
      >
        <svg class="h-2.5 w-2.5" viewBox="0 0 10 10" aria-hidden="true">
          <path d="M9 1 1 9M9 5 5 9" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" fill="none" />
        </svg>
      </div>
    </div>
  </Teleport>
</template>
