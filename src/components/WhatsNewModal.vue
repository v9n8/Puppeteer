<script setup lang="ts">
// "What's new" popup (main window, `app.whats_new`): once per app version, the
// bundled notes of the releases since the one last offered (lib/whatsNew.ts).
// The version is stored as soon as the popup opens, so closing the app before
// dismissing it does not bring it back.
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { getVersion } from '@tauri-apps/api/app'
import { invoke } from '@/lib/tauri'
import { Badge, Button, Modal } from '@/components/ui'
import { useAppSettingsStore } from '@/stores/appSettings'
import { RELEASES, SEEN_SETTING_KEY, noteText, whatsNewAtLaunch, type ChangeKind, type Release } from '@/lib/whatsNew'

const { t, locale } = useI18n()
const appSettings = useAppSettingsStore()

const version = ref('')
const notes = ref<Release[]>([])
const open = ref(false)

const KIND_TONE: Record<ChangeKind, 'success' | 'primary' | 'warning' | 'info'> = {
  added: 'success',
  improved: 'primary',
  changed: 'warning',
  fixed: 'info',
}

onMounted(async () => {
  try {
    const [current] = await Promise.all([getVersion(), appSettings.ensureLoaded()])
    const seen = appSettings.settings?.whats_new_seen_version
    const next = whatsNewAtLaunch(current, seen, RELEASES)
    if (next.markSeen) {
      await invoke('update_setting', { key: SEEN_SETTING_KEY, value: next.markSeen })
      if (appSettings.settings) appSettings.settings.whats_new_seen_version = next.markSeen
    }
    version.value = current
    notes.value = next.notes
    open.value = next.notes.length > 0
  } catch (e) {
    // Not stored: the popup is simply offered again at the next launch.
    console.warn('whats new: could not read or store the seen version', e)
  }
})
</script>

<template>
  <Modal :open="open" :title="t('whatsNew.title', { version })" size="lg" scroll-body @close="open = false">
    <div class="px-5 py-4">
      <section
        v-for="rel in notes"
        :key="rel.version"
        class="border-t border-border py-3.5 first:border-t-0 first:pt-0"
      >
        <header v-if="notes.length > 1" class="mb-2 flex items-baseline gap-2">
          <span class="text-sm font-semibold text-foreground">v{{ rel.version }}</span>
          <span class="text-xs text-muted-foreground tabular-nums">{{ rel.date }}</span>
        </header>
        <!-- Two columns: the badge column takes the widest label, so every line of
             text starts at the same edge whatever the language. -->
        <ul class="grid grid-cols-[max-content_1fr] items-start gap-x-2.5 gap-y-1.5">
          <li v-for="(item, i) in rel.items" :key="i" class="contents">
            <Badge :tone="KIND_TONE[item.kind]" size="xs" class="mt-px justify-center">
              {{ t(`whatsNew.kind.${item.kind}`) }}
            </Badge>
            <span class="text-sm text-foreground leading-snug">{{ noteText(item, locale) }}</span>
          </li>
        </ul>
      </section>
    </div>
    <template #footer>
      <Button variant="primary" @click="open = false">{{ t('whatsNew.close') }}</Button>
    </template>
  </Modal>
</template>
