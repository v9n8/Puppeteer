<script setup lang="ts">
// THE Settings window (opened via lib/settingsWindow, `?settingsWindow=1`): the
// Settings screen on its own OS window, for users who keep it beside a session
// or on a second monitor (lib/detachedScreens).
//
// It renders the very same SettingsView in `windowed` mode — no fork. This
// window has no route of the app, so the section is a ref here: first from the
// URL, then from SETTINGS_WINDOW_OPEN_SECTION (another window asked for a
// section). SettingsView draws the title bar with "Move back to main window".
//
// It is its own webview with its own Pinia: settings and accounts load fresh
// here (SettingsView's onMounted). Theme / language changed here reach the
// other windows through lib/appearanceSync. The overlay hosts the sections rely
// on (confirm dialog, text prompt, toasts) are mounted here, like PrInboxWindow.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import { ConfirmModal, PromptModal, ToastHost } from '@/components/ui'
import { refreshTray } from '@/lib/tray'
import { SETTINGS_WINDOW_OPEN_SECTION, SETTINGS_WINDOW_READY } from '@/lib/settingsWindow'
import { settingsCrumbKeys, settingsPlaceFromRoute, type SettingsPlace } from '@/lib/settingsSections'
import { SETTINGS_WINDOW_QUERY } from '@/lib/windowMode'
import SettingsView from '@/views/SettingsView.vue'

const { t, locale } = useI18n()

const place = ref<SettingsPlace>(settingsPlaceFromRoute({ section: SETTINGS_WINDOW_QUERY.section }, SETTINGS_WINDOW_QUERY))
const view = ref<InstanceType<typeof SettingsView> | null>(null)

// Native title (and the menu-bar switcher's label) follows the section and the
// app language, which App.vue applies once settings load.
const title = computed(() => settingsCrumbKeys(place.value.section).map((k) => t(k)).join(' › '))
watch(
  [title, locale],
  async () => {
    document.title = title.value
    try {
      await getCurrentWindow().setTitle(title.value)
    } catch {
      /* outside the Tauri shell */
    }
    void refreshTray()
  },
  { immediate: true },
)

const unlisten: UnlistenFn[] = []
onMounted(async () => {
  try {
    unlisten.push(
      await listen<Partial<SettingsPlace>>(SETTINGS_WINDOW_OPEN_SECTION, (e) => {
        // Re-checked: the payload crossed a window boundary.
        const p = e.payload ?? {}
        void view.value?.openPlace(settingsPlaceFromRoute({ section: p.section }, { tab: p.devicesTab, type: p.connectionsFilter }))
      }),
    )
    // Closing with unsaved Memory edits asks first (the red button, ⌘W). Unless
    // prevented here, the window is destroyed after this handler.
    unlisten.push(
      await getCurrentWindow().onCloseRequested(async (e) => {
        if (view.value && !(await view.value.mayLeave())) e.preventDefault()
      }),
    )
    await emit(SETTINGS_WINDOW_READY)
  } catch {
    /* outside the Tauri shell */
  }
})
onBeforeUnmount(() => {
  for (const off of unlisten.splice(0)) off()
})
</script>

<template>
  <div class="app-shell flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground">
    <SettingsView ref="view" v-model:place="place" windowed class="min-h-0 flex-1" />
    <ConfirmModal />
    <PromptModal />
    <ToastHost />
  </div>
</template>
