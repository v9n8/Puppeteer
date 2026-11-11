// Settings as a modal over the current screen (after AWOG's SettingsModal), so a
// setting can be changed without leaving the session. The `/settings?section=`
// route only opens it (router guard), so every old link, ⌘, and the menu keep
// working. A single <SettingsModal> host per window reads this shared state.
import { ref } from 'vue'
import { resolveSettingsPlace, type SettingsPlace } from '@/lib/settingsSections'

const open = ref(false)
/** The place a deep link asked for; the host applies it, then clears it. */
const requested = ref<SettingsPlace | null>(null)

function openSettings(section?: unknown) {
  requested.value = resolveSettingsPlace(section)
  open.value = true
}

function closeSettings() {
  open.value = false
}

export function useSettingsModal() {
  return { open, requested, openSettings, closeSettings }
}
