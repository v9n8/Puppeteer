<script setup lang="ts">
// "Search settings" box of the Settings header / the Settings window's title bar
// (V3-58): a dropdown of the rows that match, with their "Group › Section › Tab"
// path. It only picks — `pick` goes to SettingsView, which opens the place; then
// `revealSettingsRow` scrolls to the row and lights it. ⌘F / Ctrl+F focuses the
// box while Settings is on screen (the listener lives and dies with this
// component, so no other screen's find is touched).
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Search, X } from 'lucide-vue-next'
import { Kbd } from '@/components/ui'
import { useFeaturesStore } from '@/stores/features'
import { SETTINGS_SECTIONS } from '@/lib/settingsSections'
import { SETTINGS_SEARCH_ROWS } from '@/lib/settingsSearchRows'
import { resultPath, searchSettings, settingsSearchIndex, type SearchEntry } from '@/lib/settingsSearch'

const emit = defineEmits<{ pick: [entry: SearchEntry] }>()

const { t } = useI18n()
const features = useFeaturesStore()

const index = computed(() =>
  settingsSearchIndex(SETTINGS_SECTIONS, SETTINGS_SEARCH_ROWS, (k) => t(k), (k) => t(k, {}, { locale: 'en' })),
)
const input = ref<HTMLInputElement | null>(null)
const query = ref('')
const open = ref(false)
const active = ref(0)
const results = computed(() => searchSettings(index.value, query.value, features.tree))
const showPanel = computed(() => open.value && query.value.trim() !== '')
watch(results, () => { active.value = 0 })

const isMac = /mac/i.test(navigator.userAgent)
const hotkey = isMac ? '⌘F' : 'Ctrl+F'
const optionId = (i: number) => `settings-search-opt-${i}`

function pick(entry: SearchEntry) {
  open.value = false
  emit('pick', entry)
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    if (!results.value.length) return
    e.preventDefault()
    open.value = true
    const n = results.value.length
    active.value = (active.value + (e.key === 'ArrowDown' ? 1 : n - 1)) % n
    void nextTick(() => document.getElementById(optionId(active.value))?.scrollIntoView({ block: 'nearest' }))
  } else if (e.key === 'Enter') {
    const hit = results.value[active.value]
    if (showPanel.value && hit) { e.preventDefault(); pick(hit) }
  } else if (e.key === 'Escape') {
    // First Esc closes the list, the second clears the text.
    e.preventDefault()
    e.stopPropagation()
    if (showPanel.value) open.value = false
    else query.value = ''
  }
}

function clear() {
  query.value = ''
  input.value?.focus()
}

function onWindowKeydown(e: KeyboardEvent) {
  if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey || e.key.toLowerCase() !== 'f') return
  if (isMac ? !e.metaKey : !e.ctrlKey) return
  e.preventDefault()
  e.stopPropagation()
  input.value?.focus()
  input.value?.select()
  open.value = true
}
onMounted(() => window.addEventListener('keydown', onWindowKeydown, true))
onBeforeUnmount(() => window.removeEventListener('keydown', onWindowKeydown, true))
</script>

<template>
  <div class="relative w-[260px] min-w-[160px] shrink">
    <Search class="pointer-events-none absolute left-2 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" :stroke-width="1.75" aria-hidden="true" />
    <input
      ref="input"
      v-model="query"
      type="text"
      role="combobox"
      autocomplete="off"
      spellcheck="false"
      class="h-8 w-full rounded-md border border-input bg-card pl-8 pr-14 text-2xs text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      :placeholder="t('settings.search.placeholder')"
      :aria-label="t('settings.search.label')"
      aria-autocomplete="list"
      aria-controls="settings-search-list"
      :aria-expanded="showPanel"
      :aria-activedescendant="showPanel && results.length ? optionId(active) : undefined"
      @focus="open = true"
      @input="open = true"
      @blur="open = false"
      @keydown="onKeydown"
    >
    <button
      v-if="query"
      type="button"
      class="absolute right-1 top-1/2 flex h-7 w-7 -translate-y-1/2 cursor-pointer items-center justify-center rounded text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      :title="t('settings.search.clear')"
      :aria-label="t('settings.search.clear')"
      @mousedown.prevent
      @click="clear"
    >
      <X class="h-4 w-4" :stroke-width="1.75" />
    </button>
    <Kbd
      v-else
      class="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2"
    >{{ hotkey }}</Kbd>

    <div
      v-if="showPanel"
      class="absolute right-0 top-full z-30 mt-1 w-[360px] max-w-[calc(100vw-1.5rem)] overflow-hidden rounded-lg border border-border bg-popover text-popover-foreground shadow-lg"
    >
      <template v-if="results.length">
        <p class="border-b border-border/60 px-3 py-1.5 text-2xs text-muted-foreground" role="status">
          {{ t('settings.search.count', { n: results.length, q: query.trim() }) }}
        </p>
        <ul id="settings-search-list" role="listbox" :aria-label="t('settings.search.label')" class="max-h-80 overflow-auto py-1">
          <li
            v-for="(r, i) in results"
            :id="optionId(i)"
            :key="r.id"
            role="option"
            :aria-selected="i === active"
            class="flex min-h-11 cursor-pointer flex-col justify-center px-2.5 py-0"
            :class="i === active ? 'bg-accent' : 'hover:bg-accent/60'"
            @mousedown.prevent
            @mousemove="active = i"
            @click="pick(r)"
          >
            <div class="truncate text-sm text-foreground">{{ r.label }}</div>
            <div class="truncate text-2xs text-muted-foreground">{{ resultPath(r, (k) => t(k)) }}</div>
          </li>
        </ul>
        <p class="border-t border-border/60 px-3 py-1.5 text-2xs text-muted-foreground">{{ t('settings.search.footer') }}</p>
      </template>
      <div v-else class="px-3 py-4 text-center" role="status">
        <p class="text-sm font-medium text-foreground">{{ t('settings.search.none') }}</p>
        <p class="mt-1 text-2xs text-muted-foreground">{{ t('settings.search.noneHint') }}</p>
      </div>
    </div>
  </div>
</template>
