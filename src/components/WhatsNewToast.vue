<script setup lang="ts">
// "What's new" at launch (main window, `app.whats_new`): once per app version,
// when the running version has bundled notes newer than the one last offered
// (lib/whatsNew.ts), shows one toast that leads to Settings › About. Headless.
// The version is stored as soon as the toast shows, so closing the app before
// dismissing it does not bring it back.
import { onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { getVersion } from '@tauri-apps/api/app'
import { invoke } from '@/lib/tauri'
import { useToast } from '@/composables/useToast'
import { useAppSettingsStore } from '@/stores/appSettings'
import { RELEASES, SEEN_SETTING_KEY, whatsNewAtLaunch } from '@/lib/whatsNew'

const TOAST_MS = 10_000

const { t } = useI18n()
const router = useRouter()
const { toast } = useToast()
const appSettings = useAppSettingsStore()

onMounted(async () => {
  try {
    const [current] = await Promise.all([getVersion(), appSettings.ensureLoaded()])
    const seen = appSettings.settings?.whats_new_seen_version
    const next = whatsNewAtLaunch(current, seen, RELEASES)
    if (next.markSeen) {
      await invoke('update_setting', { key: SEEN_SETTING_KEY, value: next.markSeen })
      if (appSettings.settings) appSettings.settings.whats_new_seen_version = next.markSeen
    }
    if (next.notes.length === 0) return
    toast.info(t('whatsNew.toast', { version: current }), TOAST_MS, {
      label: t('whatsNew.open'),
      run: () => void router.push({ path: '/settings/about', query: { release: current } }).catch(() => {}),
    })
  } catch (e) {
    // Not stored: the toast is simply offered again at the next launch.
    console.warn('whats new: could not read or store the seen version', e)
  }
})
</script>

<template>
  <span class="hidden" aria-hidden="true" />
</template>
