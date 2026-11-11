<script setup lang="ts">
// The one selection toolbar of a window (mounted by App.vue, teleported to
// body). Selecting text — or right-clicking a selection — inside a surface
// declared with useSelectionSurface shows a small bar of the buttons that
// surface allows and the feature tree leaves on (lib/selectionSurface). Outside
// every surface, nothing shows. It also hosts the Quote popover (UX-46), opened
// from here or from the row under a turn (StreamLog).
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Languages, ListTodo, ClipboardCopy, Quote, StickyNote } from 'lucide-vue-next'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import TranslatePopover from '@/components/TranslatePopover.vue'
import QuoteNotePopover from '@/components/QuoteNotePopover.vue'
import { invoke } from '@/lib/tauri'
import { openItemCreateWindow } from '@/lib/itemWindow'
import { anchorFromRange } from '@/lib/quoteAnchor'
import {
  canTranslate,
  copyMdText,
  isSelectionExcludedWindow,
  pickSurface,
  pointToolbarPosition,
  scrollMovesSurface,
  selectionActions,
  toolbarPosition,
  type SelectionAction,
} from '@/lib/selectionSurface'
import {
  selectionBar as bar,
  selectionHost,
  selectionQuoteNote as quoteNote,
  selectionSurfaces,
  selectionTranslation as translation,
  surfaceEls,
  type SelectionSurface,
} from '@/composables/useSelectionSurface'
import { useFeaturesStore } from '@/stores/features'
import { useAppSettingsStore } from '@/stores/appSettings'

const { t } = useI18n()
const features = useFeaturesStore()
const appSettings = useAppSettingsStore()

function windowLabel(): string | null {
  try {
    return getCurrentWebviewWindow().label
  } catch {
    return null
  }
}
// App.vue does not mount the bar in the vault window; this guards it anyway.
const excludedWindow = isSelectionExcludedWindow(window.location.search, windowLabel())

const actions = computed<SelectionAction[]>(() => {
  const b = bar.value
  if (!b) return []
  return selectionActions(features.tree, b.surface.allow, {
    excludedWindow,
    hasQuoteTarget: b.quoteTarget !== null,
  })
})

const ICONS = {
  quote: Quote,
  translate: Languages,
  copyMd: ClipboardCopy,
  saveNote: StickyNote,
  saveTodo: ListTodo,
} as const

// Quote names its target on the button itself, so a quote never lands in a
// session the user did not mean.
function label(action: SelectionAction): string {
  if (action === 'quote') return t('selection.quote', { target: bar.value?.quoteTarget?.label ?? '' })
  return t(`selection.${action}`)
}

function title(action: SelectionAction): string {
  if (action === 'quote') return t('selection.quoteTitle', { target: bar.value?.quoteTarget?.label ?? '' })
  return t(`selection.${action}Title`)
}

// ── Placement ───────────────────────────────────────────────────────────────
// The bar's size is only known once rendered: measure it, then clamp.
const barEl = ref<HTMLElement | null>(null)
const barSize = ref({ width: 0, height: 0 })
const position = computed(() => {
  const b = bar.value
  if (!b) return { x: 0, y: 0 }
  const viewport = { width: window.innerWidth, height: window.innerHeight }
  return b.point
    ? pointToolbarPosition(b.point, barSize.value, viewport)
    : toolbarPosition(b.rect, barSize.value, viewport)
})

watch(actions, () => {
  void nextTick(() => {
    if (barEl.value) barSize.value = { width: barEl.value.offsetWidth, height: barEl.value.offsetHeight }
  })
})

// ── Detection ───────────────────────────────────────────────────────────────
function hide() {
  bar.value = null
}

// The bar for the current selection: under it, or at `point` when it was right-clicked.
function onSelectionEnd(point: { x: number; y: number } | null) {
  const sel = window.getSelection()
  if (!sel || sel.isCollapsed) return hide()
  const text = sel.toString().trim()
  if (text.length < 2) return hide()
  const surface = pickSurface(selectionSurfaces, surfaceEls, sel.anchorNode, sel.focusNode)
  if (!surface) return hide()
  let rect: DOMRect | null = null
  let range: Range | null = null
  try {
    range = sel.getRangeAt(0).cloneRange()
    rect = range.getBoundingClientRect()
  } catch { rect = null }
  if (!rect || (rect.width === 0 && rect.height === 0)) return hide()
  bar.value = {
    surface,
    text,
    range,
    rect: { left: rect.left, top: rect.top, bottom: rect.bottom },
    point,
    quoteTarget: surface.quoteTarget?.() ?? null,
  }
  // Warm the translation sidecar now, while the user reaches for the button, so
  // the first translation isn't paying Node/SDK cold-start.
  if (canTranslate(features.tree, surface.allow, excludedWindow)) void invoke('prewarm_translate', { source: 'selection' }).catch(() => {})
}

// Left button only: a right-click's mouseup comes after its contextmenu and
// would pull the bar back under the selection. Not while the Quote popover is
// open: a click on its Save would bring the bar back for the selection it quotes.
function onMouseUp(e: MouseEvent) {
  if (e.button === 0 && !quoteNote.value) onSelectionEnd(null)
}

// Right-clicking a selection opens the bar at the pointer instead of the
// webview's menu. Without a bar to show, the system menu shows as usual; an
// element that opened its own menu (preventDefault) keeps it alone. While the
// Quote popover is open, a right-click does nothing here.
function onContextMenu(e: MouseEvent) {
  if (quoteNote.value) return
  if (e.defaultPrevented) return hide()
  onSelectionEnd({ x: e.clientX, y: e.clientY })
  if (bar.value && actions.value.length) e.preventDefault()
}

// The bar is anchored to a viewport rect that scrolling the surface invalidates.
function onScroll(e: Event) {
  const b = bar.value
  if (b && scrollMovesSurface(e.target as Node | null, surfaceEls(b.surface), document)) hide()
}

// ── Actions ─────────────────────────────────────────────────────────────────
// Each handler runs on the selection captured when the bar was shown;
// `mousedown.prevent` on the buttons keeps the selection alive through the click.
async function run(action: SelectionAction) {
  const b = bar.value
  if (!b) return
  const at = position.value
  hide()
  switch (action) {
    case 'quote':
      // The anchor is read off the selection now, before the popover's note
      // field takes the focus.
      if (b.quoteTarget) {
        quoteNote.value = {
          surface: b.surface,
          target: b.quoteTarget,
          text: b.text,
          anchor: anchorFromRange(b.range),
          at: { left: at.x, top: at.y, bottom: at.y },
        }
      }
      break
    case 'translate':
      translation.value = { surface: b.surface, text: b.text, ...at }
      break
    case 'copyMd': {
      await navigator.clipboard.writeText(copyMdText(b.surface.markdownSource?.() ?? [], b.text)).catch(() => {})
      break
    }
    case 'saveNote':
      saveNote(b.surface, b.text)
      break
    case 'saveTodo':
      void openItemCreateWindow('todo', b.surface.capture?.().context ?? {}, { content: b.text })
      break
  }
}

function saveNote(surface: SelectionSurface, text: string) {
  const cap = surface.capture?.()
  void openItemCreateWindow('note', cap?.context ?? {}, { title: cap?.noteTitle, content: text })
}

// The open popover re-sends its text on a language switch or a retry: switching
// Translate off closes it, so nothing more reaches the model.
const openTranslation = computed(() => {
  const tr = translation.value
  return tr && canTranslate(features.tree, tr.surface.allow, excludedWindow) ? tr : null
})

const defaultTranslateLang = computed(() => appSettings.settings?.translate_target_lang || 'vi')

function closeTranslate() {
  translation.value = null
}

// Saved, the quote goes to the composer and the text it came from is no longer highlighted.
function saveQuote(note: string) {
  const q = quoteNote.value
  quoteNote.value = null
  if (!q) return
  window.getSelection()?.removeAllRanges()
  q.target.append(q.text, q.anchor, note)
}

function closeQuote() {
  quoteNote.value = null
}

onMounted(() => {
  if (excludedWindow) return
  selectionHost.value = { isOn: (id) => features.isOn(id), saveNote }
  window.addEventListener('mouseup', onMouseUp)
  window.addEventListener('contextmenu', onContextMenu)
  document.addEventListener('scroll', onScroll, true)
})
onUnmounted(() => {
  selectionHost.value = null
  window.removeEventListener('mouseup', onMouseUp)
  window.removeEventListener('contextmenu', onContextMenu)
  document.removeEventListener('scroll', onScroll, true)
  hide()
  closeTranslate()
  closeQuote()
})
</script>

<template>
  <Teleport to="body">
    <div
      v-if="bar && actions.length"
      ref="barEl"
      class="fixed z-[65] flex items-center gap-px overflow-hidden rounded-md border border-border bg-card shadow-lg shadow-black/30"
      :style="{ left: position.x + 'px', top: position.y + 'px' }"
    >
      <template v-for="(action, i) in actions" :key="action">
        <span v-if="i > 0" class="h-4 w-px bg-border" />
        <button
          class="inline-flex items-center gap-1 px-2 py-1 text-2xs font-medium hover:bg-accent/60 transition-colors cursor-pointer"
          :class="action === 'translate' ? 'text-primary' : 'text-foreground/80'"
          :title="title(action)"
          @mousedown.prevent
          @click="run(action)"
        >
          <component :is="ICONS[action]" class="h-3 w-3 shrink-0" :stroke-width="1.75" />
          <span class="max-w-[14rem] truncate">{{ label(action) }}</span>
        </button>
      </template>
    </div>
  </Teleport>

  <TranslatePopover
    v-if="openTranslation"
    :text="openTranslation.text"
    :x="openTranslation.x"
    :y="openTranslation.y"
    :initial-lang="defaultTranslateLang"
    @close="closeTranslate"
  />

  <QuoteNotePopover
    v-if="quoteNote"
    :text="quoteNote.text"
    :target="quoteNote.target.label"
    :at="quoteNote.at"
    @save="saveQuote"
    @close="closeQuote"
  />
</template>
