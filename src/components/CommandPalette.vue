<script setup lang="ts">
// Command palette (⌘K / Ctrl+K by default, prop `app.command_palette`), main
// window only. Lists what usePaletteItems offers, laid out by the user's config
// (pinned first, then the groups in their order) and filtered by
// lib/commandPalette's accent-insensitive fuzzy search. Pin / hide act on the
// item's id.
//
// With text typed, a last row searches it in every session (Search sessions,
// ⌘⇧F). Opened by the View menu item `view.commandPalette`, whose shortcut is in the
// keymap (lib/keymap) and fired by the OS. Without the native menu (a browser)
// the same shortcut is caught here instead.
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { EyeOff, Pin, PinOff, Search } from 'lucide-vue-next'
import { Modal } from '@/components/ui'
import { IS_MAC, effectiveKeymap, hasNativeMenu, menuAccelerator, registerMenuAction } from '@/lib/appMenu'
import { errorText } from '@/lib/featureGate'
import { hideItem, paletteSections, searchAllSection, togglePinned } from '@/lib/commandPalette'
import { PALETTE_ACTION, formatAccelerator, matchesAccelerator } from '@/lib/keymap'
import { useFeaturesStore } from '@/stores/features'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useCommandPaletteStore } from '@/stores/commandPalette'
import { usePaletteItems, type PaletteCommand } from '@/composables/usePaletteItems'
import { useToast } from '@/composables/useToast'

const { t } = useI18n()
const router = useRouter()
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
const SEARCH_ACTION = 'go./search'
const searchSection = computed(() => searchAllSection(query.value, features.tree))
const searchRow = computed<PaletteCommand | null>(() => {
  const item = searchSection.value?.items[0]
  if (!item) return null
  return {
    id: item.id,
    group: 'nav',
    label: t('palette.searchAll', { q: item.query }),
    icon: Search,
    run: () => openSearch(item.query),
  }
})
const searchShortcut = computed(() => {
  const accel = effectiveKeymap().get(SEARCH_ACTION)
  return accel ? formatAccelerator(accel, IS_MAC) : ''
})
// Menu-backed rows (`window.openWindows`, `file.newSession`…) share their id with the keymap action.
function rowShortcut(id: string): string {
  const accel = effectiveKeymap().get(id)
  return accel ? formatAccelerator(accel, IS_MAC) : ''
}
// Rows in screen order, for ↑ ↓ across sections; the search row is last.
const rows = computed(() => [...sections.value.flatMap((s) => s.items), ...(searchRow.value ? [searchRow.value] : [])])
const pinned = computed(() => new Set(palette.config.pinned))

function rowIndex(item: PaletteCommand): number {
  return rows.value.indexOf(item)
}

function openSearch(q: string) {
  router.push({ path: '/search', query: { q } }).catch(() => {})
}

// While open, the Search sessions shortcut takes the typed text along.
let unbindSearchAction: (() => void) | null = null
function searchTyped() {
  close()
  const q = query.value.trim()
  if (q) openSearch(q)
  else router.push('/search').catch(() => {})
}

function show() {
  open.value = true
  unbindSearchAction?.()
  unbindSearchAction = registerMenuAction(SEARCH_ACTION, searchTyped)
  query.value = ''
  active.value = 0
  // The layout may have changed in another window since.
  appSettings.refresh().catch(() => {})
  nextTick(() => inputEl.value?.focus())
}

function close() {
  open.value = false
  unbindSearchAction?.()
  unbindSearchAction = null
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

function toggle() {
  if (open.value) {
    close()
    return
  }
  // Not over another dialog: it owns the keyboard.
  if (document.querySelector('.ui-backdrop')) return
  show()
}

// With the native menu its accelerator fires instead: handling the key here
// too would open and close the palette on one press.
function onKeydown(e: KeyboardEvent) {
  if (hasNativeMenu() || e.defaultPrevented || e.isComposing) return
  const searchAccel = menuAccelerator(SEARCH_ACTION)
  if (open.value && searchAccel && matchesAccelerator(e, searchAccel, IS_MAC)) {
    e.preventDefault()
    searchTyped()
    return
  }
  const accel = menuAccelerator(PALETTE_ACTION)
  if (!accel || !matchesAccelerator(e, accel, IS_MAC)) return
  e.preventDefault()
  toggle()
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

let unbindMenuAction: (() => void) | null = null
onMounted(() => {
  unbindMenuAction = registerMenuAction(PALETTE_ACTION, toggle)
  window.addEventListener('keydown', onKeydown)
})
onBeforeUnmount(() => {
  unbindMenuAction?.()
  unbindSearchAction?.()
  window.removeEventListener('keydown', onKeydown)
})
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
            <kbd v-if="rowShortcut(item.id)" class="shrink-0 rounded border border-border/70 px-1 font-sans text-2xs text-muted-foreground">{{ rowShortcut(item.id) }}</kbd>
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
        <section v-if="searchSection && searchRow" class="mb-1 last:mb-0">
          <h4 class="px-2.5 pb-1 pt-2 text-2xs font-semibold uppercase tracking-wide text-muted-foreground">
            {{ t('palette.group.search') }}
          </h4>
          <div
            :ref="(el) => { if (el) rowEls[rowIndex(searchRow!)] = el as HTMLElement }"
            role="option"
            :aria-selected="active === rowIndex(searchRow)"
            class="flex cursor-pointer items-center gap-2.5 rounded px-2.5 py-1.5 transition-colors"
            :class="active === rowIndex(searchRow) ? 'bg-accent' : 'hover:bg-accent/60'"
            @mouseenter="active = rowIndex(searchRow!)"
            @click="run(searchRow!)"
          >
            <Search class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
            <span class="min-w-0 flex-1 truncate text-sm text-popover-foreground">{{ searchRow.label }}</span>
            <kbd v-if="searchShortcut" class="shrink-0 rounded border border-border/70 px-1 font-sans text-2xs text-muted-foreground">{{ searchShortcut }}</kbd>
          </div>
        </section>
      </div>

      <p class="border-t border-border px-3 py-1.5 text-2xs text-muted-foreground">
        {{ t('palette.hint') }}
      </p>
    </div>
  </Modal>
</template>
