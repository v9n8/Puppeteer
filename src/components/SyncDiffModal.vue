<script setup lang="ts">
// Two versions of one file side by side (V3-47, reused by V3-50): the project's
// copy on the left, the central one on the right. A side that could not be read
// shows its own error and the other side still shows. The action buttons come in
// the `footer` slot. Focus stays inside while open and returns to the opener.
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertCircle } from 'lucide-vue-next'
import { Banner, Modal } from '@/components/ui'
import DiffView from '@/components/DiffView.vue'

export interface SyncDiffSide {
  /** Heading of the column ("Project copy"). */
  label: string
  path: string
  /** Null when the side could not be read; see `error`. */
  content: string | null
  error: string | null
}

const props = defineProps<{
  open: boolean
  title: string
  /** Left: the project's copy. Right: the central one. */
  left: SyncDiffSide
  right: SyncDiffSide
  loading?: boolean
}>()
const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()

// Below this width the two columns would be too narrow to read: one list instead.
const NARROW_PX = 720
const narrow = ref(false)
function measure() { narrow.value = window.innerWidth < NARROW_PX }
onMounted(() => { measure(); window.addEventListener('resize', measure) })
onBeforeUnmount(() => window.removeEventListener('resize', measure))

const bothRead = computed(() => props.left.content !== null && props.right.content !== null)

// --- Focus: into the dialog on open, trapped, back to the opener on close ---
const root = ref<HTMLElement | null>(null)
let opener: HTMLElement | null = null
const FOCUSABLE = 'button:not([disabled]), [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'

watch(() => props.open, async (open) => {
  if (open) {
    opener = document.activeElement instanceof HTMLElement ? document.activeElement : null
    await nextTick()
    root.value?.focus()
  } else {
    opener?.focus()
    opener = null
  }
}, { immediate: true })

function onKeydown(e: KeyboardEvent) {
  if (e.key !== 'Tab' || !root.value) return
  const items = [...root.value.querySelectorAll<HTMLElement>(FOCUSABLE)]
  if (items.length === 0) { e.preventDefault(); return }
  const first = items[0]
  const last = items[items.length - 1]
  const active = document.activeElement
  if (e.shiftKey && (active === first || active === root.value)) { e.preventDefault(); last.focus() }
  else if (!e.shiftKey && active === last) { e.preventDefault(); first.focus() }
}
</script>

<template>
  <Modal :open="open" size="2xl" hide-header @close="emit('close')">
    <div ref="root" tabindex="-1" class="flex max-h-[86vh] flex-col outline-none" role="dialog" aria-modal="true" :aria-label="title" @keydown="onKeydown">
      <div class="flex shrink-0 items-center gap-2 border-b border-border px-4 py-3">
        <h3 class="min-w-0 flex-1 truncate font-mono text-sm font-semibold" :title="title">{{ title }}</h3>
        <button
          type="button"
          class="h-7 shrink-0 cursor-pointer rounded-md px-2.5 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          @click="emit('close')"
        >{{ t('projectDetail.close') }}</button>
      </div>

      <div class="min-h-0 flex-1 space-y-3 overflow-auto p-4">
        <p v-if="loading" class="py-6 text-center text-xs text-muted-foreground">{{ t('projectDetail.diffLoading') }}</p>
        <template v-else>
          <!-- Column headings: label and full path on hover -->
          <div class="grid gap-2 text-xs" :class="narrow ? 'grid-cols-1' : 'grid-cols-2'">
            <div v-for="side in [left, right]" :key="side.label" class="min-w-0">
              <p class="font-medium">{{ side.label }}</p>
              <p class="truncate font-mono text-2xs text-muted-foreground" :title="side.path">{{ side.path }}</p>
            </div>
          </div>

          <DiffView v-if="bothRead" :before="left.content ?? ''" :after="right.content ?? ''" :split="!narrow" />

          <!-- One side unreadable: show that side's reason, the other side as plain text -->
          <div v-else class="grid gap-2" :class="narrow ? 'grid-cols-1' : 'grid-cols-2'">
            <div v-for="side in [left, right]" :key="side.label" class="min-w-0">
              <Banner v-if="side.content === null" tone="destructive" :icon="AlertCircle">
                <span class="break-words">{{ side.error ?? t('projectDetail.unreadable') }}</span>
              </Banner>
              <pre
                v-else
                class="max-h-[28rem] overflow-auto whitespace-pre-wrap break-words rounded-md border border-border p-2 font-mono text-2xs leading-relaxed"
              >{{ side.content }}</pre>
            </div>
          </div>
        </template>
      </div>

      <div v-if="$slots.footer" class="flex shrink-0 flex-wrap items-center justify-end gap-2 border-t border-border px-4 py-3">
        <slot name="footer" />
      </div>
    </div>
  </Modal>
</template>
