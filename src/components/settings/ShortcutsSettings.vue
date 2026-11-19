<script setup lang="ts">
// Settings → Shortcuts & quick commands: the ⌘K palette and the menu's shortcuts
// as two tabs. The tab is the model, so the URL (`?tab=`) holds it.
import { nextTick, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import CommandPaletteSettings from '@/components/CommandPaletteSettings.vue'
import KeymapSettings from '@/components/KeymapSettings.vue'

export type ShortcutsTab = 'palette' | 'keymap'

const TABS: ShortcutsTab[] = ['palette', 'keymap']

const { t } = useI18n()
const tab = defineModel<ShortcutsTab>('tab', { default: 'palette' })
const buttons = ref<HTMLButtonElement[]>([])

// ← → move between tabs and select, like a native tab list.
async function onKey(e: KeyboardEvent) {
  if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return
  e.preventDefault()
  const step = e.key === 'ArrowRight' ? 1 : -1
  const next = TABS[(TABS.indexOf(tab.value) + step + TABS.length) % TABS.length]
  tab.value = next
  await nextTick()
  buttons.value[TABS.indexOf(next)]?.focus()
}
</script>

<template>
  <div class="space-y-4">
    <div class="flex border-b border-border/60 text-xs font-medium" role="tablist" @keydown="onKey">
      <button
        v-for="id in TABS"
        :key="id"
        ref="buttons"
        :data-settings-id="id === 'palette' ? 'shortcuts-palette' : 'shortcuts-keymap'"
        type="button"
        role="tab"
        :aria-selected="tab === id"
        :tabindex="tab === id ? 0 : -1"
        class="h-8 px-3 transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        :class="tab === id ? 'text-foreground border-b-2 border-primary' : 'text-muted-foreground hover:text-foreground'"
        @click="tab = id"
      >
        {{ t(`settings.shortcuts.tabs.${id}`) }}
      </button>
    </div>

    <CommandPaletteSettings v-if="tab === 'palette'" />
    <KeymapSettings v-else />
  </div>
</template>
