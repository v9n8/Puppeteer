import { defineStore } from 'pinia'
import { computed } from 'vue'
import { invoke } from '@/lib/tauri'
import { useAppSettingsStore } from '@/stores/appSettings'
import {
  PALETTE_SETTING_KEY,
  parsePaletteConfig,
  serializePaletteConfig,
  type PaletteConfig,
} from '@/lib/commandPalette'

/**
 * The command palette's layout (the `palette.config` row, read through
 * `get_settings`). Shared by the palette and Settings → Command palette, so a
 * change in one shows in the other at once.
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

  return { config, save }
})
