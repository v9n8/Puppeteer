<script setup lang="ts">
// Command palette (⌘K / Ctrl+K, prop `app.command_palette`), main window only.
// Lists what usePaletteItems offers, laid out by the user's config (pinned
// first, then the groups in their order) and filtered by lib/commandPalette's
// accent-insensitive fuzzy search. Pin / hide act on the item's id.
//
// No native menu accelerator: the shortcut is caught here, like ⌘F
// (usePageFind `useFindShortcut`).
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { EyeOff, Pin, PinOff, Search } from 'lucide-vue-next'
import { Modal } from '@/components/ui'
import { IS_MAC } from '@/lib/appMenu'
import { errorText } from '@/lib/featureGate'
import { hideItem, isPaletteShortcut, paletteSections, togglePinned } from '@/lib/commandPalette'
import { useFeaturesStore } from '@/stores/features'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useCommandPaletteStore } from '@/stores/commandPalette'
import { usePaletteItems, type PaletteCommand } from '@/composables/usePaletteItems'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const features = useFeaturesStore()
const appSettings = useAppSettingsStore()
const palette = useCommandPaletteStore()
const { items } = usePaletteItems()
const { toast } = useToast()

const open = ref(false)
const query = ref('')
const active = ref(0)
const inputEl = ref<HTMLInputElement | null>(null)
const rowEls = ref<HTMLElement[]>([])

const sections = computed(() => paletteSections(items.value, palette.config, features.tree, query.value))
// Rows in screen order, for ↑ ↓ across sections.
const rows = computed(() => sections.value.flatMap((s) => s.items))
const pinned = computed(() => new Set(palette.config.pinned))

function rowIndex(item: PaletteCommand): number {
  return rows.value.indexOf(item)
}

function show() {
  open.value = true
  query.value = ''
  active.value = 0
  // The prompts and the layout may have changed in another window since.
  appSettings.refresh().catch(() => {})
  nextTick(() => inputEl.value?.focus())
}

function close() {
  open.value = false
}

function run(item: PaletteCommand | undefined) {
  if (!item) return
  close()
  void Promise.resolve(item.run()).catch((e) => toast.error(errorText(e, t)))
}

async function saveConfig(next: Parameters<typeof palette.save>[0]) {
  try {
    await palette.save(next)
  } catch (e) {
    toast.error(errorText(e, t))
  }
  inputEl.value?.focus()
}

function onInputKeydown(e: KeyboardEvent) {
  if (e.isComposing || e.keyCode === 229) return
  const n = rows.value.length
  if (e.key === 'ArrowDown' && n) {
    e.preventDefault()
    active.value = (active.value + 1) % n
  } else if (e.key === 'ArrowUp' && n) {
    e.preventDefault()
    active.value = (active.value - 1 + n) % n
  } else if (e.key === 'Enter') {
    e.preventDefault()
    run(rows.value[active.value])
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.defaultPrevented || e.isComposing || !isPaletteShortcut(e, IS_MAC)) return
  if (open.value) {
    e.preventDefault()
    close()
    return
  }
  // Not over another dialog: it owns the keyboard.
  if (document.querySelector('.ui-backdrop')) return
  e.preventDefault()
  show()
}

watch(query, () => {
  active.value = 0
})
watch(rows, (list) => {
  if (active.value >= list.length) active.value = Math.max(0, list.length - 1)
})
watch(active, (i) => {
  rowEls.value[i]?.scrollIntoView({ block: 'nearest' })
})

onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <Modal :open="open" size="md" hide-header @close="close">
    <div class="flex max-h-[70vh] flex-col">
      <div class="relative border-b border-border">
        <Search
          class="pointer-events-none absolute left-3.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground"
          :stroke-width="1.75"
        />
        <input
          ref="inputEl"
          v-model="query"
          type="text"
          class="h-11 w-full bg-transparent pl-10 pr-4 text-sm text-foreground outline-none placeholder:text-muted-foreground"
          :placeholder="t('palette.placeholder')"
          :aria-label="t('palette.placeholder')"
          @keydown="onInputKeydown"
        >
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto p-1.5">
        <p v-if="!rows.length" class="px-3 py-6 text-center text-xs text-muted-foreground">
          {{ t('palette.noResults') }}
        </p>
        <section v-for="section in sections" :key="section.id" class="mb-1 last:mb-0">
          <h4 class="px-2.5 pb-1 pt-2 text-2xs font-semibold uppercase tracking-wide text-muted-foreground">
            {{ t(`palette.group.${section.id}`) }}
          </h4>
          <div
            v-for="item in section.items"
            :key="item.id"
            :ref="(el) => { if (el) rowEls[rowIndex(item)] = el as HTMLElement }"
            role="option"
            :aria-selected="active === rowIndex(item)"
            class="group flex cursor-pointer items-center gap-2.5 rounded px-2.5 py-1.5 transition-colors"
            :class="active === rowIndex(item) ? 'bg-accent' : 'hover:bg-accent/60'"
            @mouseenter="active = rowIndex(item)"
            @click="run(item)"
          >
            <component :is="item.icon" class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
            <span class="min-w-0 flex-1 truncate text-sm text-popover-foreground">
              {{ item.label }}
              <span v-if="item.detail" class="ml-1.5 text-2xs text-muted-foreground">{{ item.detail }}</span>
            </span>
            <button
              type="button"
              class="flex h-6 w-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-background hover:text-foreground"
              :class="pinned.has(item.id) || active === rowIndex(item) ? 'visible' : 'invisible group-hover:visible'"
              :title="pinned.has(item.id) ? t('palette.unpin') : t('palette.pin')"
              :aria-label="pinned.has(item.id) ? t('palette.unpin') : t('palette.pin')"
              @click.stop="saveConfig(togglePinned(palette.config, item.id))"
            >
              <component :is="pinned.has(item.id) ? PinOff : Pin" class="h-3.5 w-3.5" :stroke-width="1.75" />
            </button>
            <button
              type="button"
              class="flex h-6 w-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-background hover:text-foreground"
              :class="active === rowIndex(item) ? 'visible' : 'invisible group-hover:visible'"
              :title="t('palette.hide')"
              :aria-label="t('palette.hide')"
              @click.stop="saveConfig(hideItem(palette.config, item.id))"
            >
              <EyeOff class="h-3.5 w-3.5" :stroke-width="1.75" />
            </button>
          </div>
        </section>
      </div>

      <p class="border-t border-border px-3 py-1.5 text-2xs text-muted-foreground">
        {{ t('palette.hint') }}
      </p>
    </div>
  </Modal>
</template>
