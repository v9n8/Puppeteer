import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { invoke } from '@/lib/tauri'
import { useAppSettingsStore } from '@/stores/appSettings'
import {
  PALETTE_SETTING_KEY,
  parsePaletteConfig,
  serializePaletteConfig,
  type PaletteConfig,
} from '@/lib/commandPalette'

type ComposerInsert = (text: string) => void

/**
 * The command palette's layout (the `palette.config` row, read through
 * `get_settings`) and the link to the composer on screen, which runs a saved
 * prompt picked from the palette. Shared by the palette and Settings → Command
 * palette, so a change in one shows in the other at once.
 */
export const useCommandPaletteStore = defineStore('commandPalette', () => {
  const appSettings = useAppSettingsStore()

  const config = computed(() => parsePaletteConfig(appSettings.settings?.palette_config))

  /** Write the layout. Shown at once; reloaded from the backend if the write fails. */
  async function save(next: PaletteConfig): Promise<void> {
    const value = serializePaletteConfig(next)
    if (appSettings.settings) appSettings.settings.palette_config = value
    try {
      await invoke('update_setting', { key: PALETTE_SETTING_KEY, value })
    } catch (e) {
      await appSettings.refresh().catch(() => {})
      throw e
    }
  }

  // The composer of the session on screen (RunView), if any.
  const composer = ref<ComposerInsert | null>(null)

  /** Called by the screen holding a composer; returns the unbind function. */
  function bindComposer(insert: ComposerInsert): () => void {
    composer.value = insert
    return () => {
      if (composer.value === insert) composer.value = null
    }
  }

  /** Put `text` in the composer on screen; false when there is none. */
  function insertIntoComposer(text: string): boolean {
    if (!composer.value) return false
    composer.value(text)
    return true
  }

  return { config, save, bindComposer, insertIntoComposer }
})
