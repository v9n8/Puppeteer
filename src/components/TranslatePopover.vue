<script setup lang="ts">
// Floating result card for the "translate selection" feature. Given the selected
// text and a viewport anchor, it calls the `translate_text` backend command
// (one-shot LLM through the run sidecar) and renders the translation as markdown.
// It is opened by the shared selection toolbar (SelectionToolbar.vue); this
// component owns the request lifecycle, the language quick-toggle, copy, and
// dismissal.
import { ref, onMounted, onUnmounted, computed, nextTick, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Languages, Copy, Check, X, Loader2, RefreshCw } from 'lucide-vue-next'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@/lib/tauri'
import { useMarkdown } from '@/lib/markdown'
import { errorText } from '@/lib/featureGate'

const { t } = useI18n()

const props = defineProps<{
  text: string
  x: number
  y: number
  initialLang: string
}>()

const emit = defineEmits<{ (e: 'close'): void }>()

const LANG_OPTIONS = [
  { value: 'vi', label: 'Tiếng Việt' },
  { value: 'en', label: 'English' },
  { value: 'ja', label: '日本語' },
  { value: 'zh', label: '中文' },
  { value: 'ko', label: '한국어' },
]

const { renderText, loadMarkdown } = useMarkdown()

const rootEl = ref<HTMLElement | null>(null)
const lang = ref(props.initialLang || 'vi')
const phase = ref<'loading' | 'streaming' | 'done' | 'error'>('loading')
const result = ref('')
const errorMsg = ref('')
const copied = ref(false)

// The turn id of the in-flight translation; streamed events tagged with a
// different id belong to a superseded turn and are ignored.
let currentTurn = -1

// Keep the card inside the viewport: clamp to the right/bottom edges.
const POPOVER_WIDTH = 380
const style = computed(() => {
  const left = Math.max(8, Math.min(props.x, window.innerWidth - POPOVER_WIDTH - 8))
  const top = Math.max(8, Math.min(props.y, window.innerHeight - 80))
  return { left: `${left}px`, top: `${top}px`, width: `${POPOVER_WIDTH}px` }
})

const renderedHtml = computed(() => (result.value ? renderText(result.value) : ''))

// Streamed translation events (shared channel; filtered by turn id).
const unlisteners: UnlistenFn[] = []

async function runTranslate() {
  phase.value = 'loading'
  errorMsg.value = ''
  result.value = ''
  currentTurn = -1
  try {
    // `selection`: the backend refuses it while the bar's translate switch is off.
    const turn = await invoke<number>('translate_text', {
      text: props.text,
      targetLang: lang.value,
      source: 'selection',
    })
    currentTurn = turn
  } catch (e) {
    errorMsg.value = errorText(e, t)
    phase.value = 'error'
  }
}

function onChunk(turn: number, delta: string) {
  if (turn !== currentTurn) return
  result.value += delta
  if (phase.value === 'loading') phase.value = 'streaming'
}

function onDone(turn: number, text: string) {
  if (turn !== currentTurn) return
  result.value = text
  phase.value = 'done'
}

function onError(turn: number, error: string) {
  if (turn !== currentTurn) return
  errorMsg.value = error
  phase.value = 'error'
}

const failedText = computed(() => t('misc.translate.failed', { error: errorMsg.value }))

function setLang(next: string) {
  if (next === lang.value) return
  lang.value = next
  runTranslate()
}

async function copyResult() {
  if (!result.value) return
  try {
    await navigator.clipboard.writeText(result.value)
    copied.value = true
    setTimeout(() => { copied.value = false }, 1500)
  } catch {
    // clipboard denied — ignore
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') { e.stopPropagation(); emit('close') }
}

function onDocMouseDown(e: MouseEvent) {
  if (rootEl.value && !rootEl.value.contains(e.target as Node)) emit('close')
}

onMounted(async () => {
  loadMarkdown()
  // Subscribe before kicking off the turn so no early chunk is missed.
  unlisteners.push(
    await listen<{ turn: number; delta: string }>('translate:chunk', (e) =>
      onChunk(e.payload.turn, e.payload.delta),
    ),
    await listen<{ turn: number; text: string }>('translate:done', (e) =>
      onDone(e.payload.turn, e.payload.text),
    ),
    await listen<{ turn: number; error: string }>('translate:error', (e) =>
      onError(e.payload.turn, e.payload.error),
    ),
  )
  runTranslate()
  nextTick(() => {
    document.addEventListener('keydown', onKeydown, true)
    // Defer the outside-click listener so the click that opened us doesn't close it.
    setTimeout(() => document.addEventListener('mousedown', onDocMouseDown, true), 0)
  })
})

onUnmounted(() => {
  document.removeEventListener('keydown', onKeydown, true)
  document.removeEventListener('mousedown', onDocMouseDown, true)
  for (const un of unlisteners) un()
  // Stop the in-flight turn but keep the warm sidecar hot for the next selection.
  void invoke('cancel_translate').catch(() => {})
})

// If the parent swaps the selected text while open, re-translate.
watch(() => props.text, () => runTranslate())
</script>

<template>
  <Teleport to="body">
    <div
      ref="rootEl"
      class="fixed z-[70] rounded-xl border border-border bg-card shadow-xl shadow-black/30 overflow-hidden flex flex-col"
      :style="style"
    >
      <!-- Header -->
      <div class="flex items-center gap-2 px-3 py-2 border-b border-border bg-muted/40">
        <Languages class="h-3.5 w-3.5 text-primary/80 shrink-0" :stroke-width="1.75" />
        <span class="text-2xs font-semibold text-foreground/80">{{ t('misc.translate.heading') }}</span>
        <!-- State of the translation: the body only ever holds the translated text. -->
        <div class="ml-2 flex min-w-0 flex-1 items-center justify-end gap-1.5 text-2xs" aria-live="polite">
          <span v-if="phase === 'loading'" class="flex items-center gap-1 text-foreground/50">
            <Loader2 class="h-3 w-3 animate-spin" :stroke-width="2" />{{ t('misc.translate.translating') }}
          </span>
          <template v-else-if="phase === 'error'">
            <span class="min-w-0 truncate text-destructive" :title="failedText">{{ failedText }}</span>
            <button
              class="inline-flex h-7 shrink-0 items-center gap-1 rounded px-1.5 text-primary hover:bg-primary/10 cursor-pointer"
              @click="runTranslate"
            >
              <RefreshCw class="h-3 w-3" :stroke-width="2" />{{ t('misc.translate.retry') }}
            </button>
          </template>
        </div>
        <div class="flex items-center gap-0.5">
          <button
            v-for="opt in LANG_OPTIONS"
            :key="opt.value"
            class="text-2xs px-1.5 py-0.5 rounded transition-colors cursor-pointer"
            :class="opt.value === lang
              ? 'bg-primary/15 text-primary font-medium'
              : 'text-foreground/50 hover:text-foreground/80 hover:bg-foreground/5'"
            @click="setLang(opt.value)"
          >{{ opt.label }}</button>
        </div>
        <button
          v-if="phase === 'done'"
          class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded text-foreground/50 hover:bg-foreground/5 hover:text-foreground cursor-pointer"
          :title="copied ? t('misc.translate.copied') : t('misc.translate.copy')"
          :aria-label="t('misc.translate.copy')"
          @click="copyResult"
        >
          <component :is="copied ? Check : Copy" class="h-3.5 w-3.5" :stroke-width="1.75" />
        </button>
        <button
          class="shrink-0 text-foreground/40 hover:text-foreground/80 transition-colors cursor-pointer"
          :title="t('misc.translate.close')"
          @click="emit('close')"
        >
          <X class="h-3.5 w-3.5" :stroke-width="2" />
        </button>
      </div>

      <!-- Body -->
      <div class="px-3 py-2.5 max-h-[50vh] overflow-auto">
        <div v-if="phase === 'loading'" class="space-y-2 py-1" aria-hidden="true">
          <div class="h-3 w-full animate-pulse rounded bg-foreground/10" />
          <div class="h-3 w-5/6 animate-pulse rounded bg-foreground/10" />
          <div class="h-3 w-2/3 animate-pulse rounded bg-foreground/10" />
        </div>
        <p v-else-if="phase === 'error'" class="py-2 text-xs text-foreground/50">{{ t('misc.translate.empty') }}</p>
        <div
          v-else
          v-html="renderedHtml"
          class="markdown-output text-sm leading-relaxed text-foreground"
        />
      </div>
    </div>
  </Teleport>
</template>
