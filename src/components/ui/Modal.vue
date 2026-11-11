<script setup lang="ts">
// Shared dialog: dimmed overlay + centered panel, Esc / overlay-click to close,
// optional title + close button + footer. Replaces the hand-rolled
// `fixed inset-0 z-50 …` overlays repeated across the app.
import { computed, ref, watch, onBeforeUnmount, useSlots } from 'vue'
import { X } from 'lucide-vue-next'
import { cn } from '@/lib/utils'
import { isTopModal, pushModal, removeModal } from '@/lib/modalStack'

const props = withDefaults(defineProps<{
  open: boolean
  title?: string
  size?: 'sm' | 'md' | 'lg' | 'xl' | '2xl' | 'full'
  /** When false, hides the X and disables Esc / overlay-click dismissal. */
  closable?: boolean
  /** When false, clicking the dimmed overlay won't close it (X / Esc still do).
   *  Mirrors Drawer: a stray click must not discard a half-typed edit. */
  dismissOnOverlay?: boolean
  /** Caps panel height and makes the body scroll. */
  scrollBody?: boolean
  /** Suppress the built-in header (the body provides its own). Esc / overlay
   *  dismissal still work when `closable`. */
  hideHeader?: boolean
}>(), {
  size: 'md',
  closable: true,
  dismissOnOverlay: true,
  scrollBody: false,
  hideHeader: false,
})

const emit = defineEmits<{ close: []; 'update:open': [value: boolean] }>()
const slots = useSlots()

const SIZES = { sm: 'max-w-md', md: 'max-w-lg', lg: 'max-w-2xl', xl: 'max-w-3xl', '2xl': 'max-w-[1100px]', full: 'max-w-[96vw]' }
// `2xl` and `full` keep a fixed height with `scrollBody`, so a body that switches
// tabs doesn't make the panel jump; the others fit their content up to 80vh.
const panelClass = computed(() =>
  cn(
    'ui-modal w-full rounded-lg border border-popover-border bg-popover shadow-2xl shadow-black/30',
    SIZES[props.size],
    props.scrollBody &&
      (props.size === 'full'
        ? 'h-[94vh] flex flex-col overflow-hidden'
        : props.size === '2xl'
          ? 'h-[86vh] flex flex-col overflow-hidden'
          : 'max-h-[80vh] flex flex-col overflow-hidden'),
  ),
)

function close() {
  if (!props.closable) return
  emit('close')
  emit('update:open', false)
}

function onOverlayClick() {
  if (!props.dismissOnOverlay) return
  close()
}

const modalId = Symbol('modal')
const zIndex = ref(50)

// Only the newest open dialog takes Escape (lib/modalStack).
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape' && isTopModal(modalId)) close()
}

watch(
  () => props.open,
  (open) => {
    if (open) {
      zIndex.value = pushModal(modalId)
      window.addEventListener('keydown', onKey)
    } else {
      removeModal(modalId)
      window.removeEventListener('keydown', onKey)
    }
  },
  { immediate: true },
)
onBeforeUnmount(() => {
  removeModal(modalId)
  window.removeEventListener('keydown', onKey)
})
</script>

<template>
  <Teleport to="body">
  <div
    v-if="open"
    class="ui-backdrop fixed inset-0 flex items-center justify-center bg-black/60 p-4"
    :style="{ zIndex }"
    @click.self="onOverlayClick"
  >
    <div :class="panelClass">
      <!-- Header -->
      <div
        v-if="!hideHeader && (title || slots.header || closable)"
        class="flex items-center gap-2 border-b border-border px-4 py-3 shrink-0"
      >
        <slot name="header">
          <h3 class="text-sm font-semibold flex-1">{{ title }}</h3>
        </slot>
        <button
          v-if="closable"
          class="flex items-center justify-center h-6 w-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors cursor-pointer shrink-0"
          aria-label="Close"
          title="Close (Esc)"
          @click="close"
        >
          <X class="h-4 w-4" :stroke-width="1.75" />
        </button>
      </div>

      <!-- Body -->
      <div :class="scrollBody ? 'flex-1 overflow-auto min-h-0' : ''">
        <slot />
      </div>

      <!-- Footer -->
      <div v-if="slots.footer" class="flex items-center justify-end gap-2 border-t border-border px-4 py-3 shrink-0">
        <slot name="footer" />
      </div>
    </div>
  </div>
  </Teleport>
</template>
