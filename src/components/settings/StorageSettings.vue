<script setup lang="ts">
// Settings → System › "Storage": the age after which old files are cleaned
// automatically (`storage_cleanup_days`, `storage_cleanup`), and the last
// cleanup. Per machine: Remote Control cannot change it and a backup leaves it
// out. What is on disk, and "Clean now", live on the Stats page.
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { RouterLink } from 'vue-router'
import { HardDrive } from 'lucide-vue-next'
import { Card, Input } from '@/components/ui'
import { errorText } from '@/lib/featureGate'
import { formatSize } from '@/lib/ipc/nativeCore'
import { formatLast, isValidDays, type LastCleanup } from '@/lib/storageCleanup'
import { previewStorageCleanup } from '@/stores/stats'
import { useFeaturesStore } from '@/stores/features'
import { useSettingsForm } from './useSettingsForm'
import SettingsRow from './SettingsRow.vue'

const { t } = useI18n()
const { settings } = useSettingsForm()
const features = useFeaturesStore()

const value = computed(() => String(settings.value.storage_cleanup_days ?? '').trim())
// The backend takes anything else as the default; say so instead of failing quietly.
const invalid = computed(() => value.value !== '' && !isValidDays(value.value))
const off = computed(() => value.value === '0' || !features.isOn('app.storage_cleanup'))

const last = ref<LastCleanup | null>(null)
const lastLoaded = ref(false)
// "No cleanup yet" would be false when the preview itself failed.
const lastError = ref('')
const lastText = computed(() => {
  const l = formatLast(last.value)
  if (!l) return t('storageCleanup.settings.never')
  const text = t('storageCleanup.settings.lastSummary', {
    when: l.at.toLocaleString(),
    trigger: t(`storageCleanup.settings.trigger.${l.trigger}`),
    files: l.files.toLocaleString(),
    size: formatSize(l.bytes),
  })
  return l.errors ? `${text} · ${t('storageCleanup.settings.lastErrors', { n: l.errors })}` : text
})

onMounted(async () => {
  try {
    last.value = (await previewStorageCleanup()).last
  } catch (e) {
    console.warn('[storage-settings] preview failed', e)
    lastError.value = t('storageCleanup.previewFailed', { error: errorText(e, t) })
  } finally {
    lastLoaded.value = true
  }
})
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <HardDrive class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('storageCleanup.settings.title') }}</span>
    </template>

    <SettingsRow search-id="storage-days" :label="t('storageCleanup.settings.days')" :hint="t('storageCleanup.settings.daysHint')">
      <Input v-model="settings.storage_cleanup_days" type="number" min="0" max="3650" placeholder="30" />
    </SettingsRow>
    <p class="text-2xs leading-relaxed" :class="invalid ? 'text-warning' : 'text-muted-foreground'">
      {{ invalid ? t('storageCleanup.settings.invalid') : off ? t('storageCleanup.settings.off') : t('storageCleanup.settings.range') }}
    </p>

    <div class="space-y-1 text-xs" data-settings-id="storage-last">
      <p class="text-muted-foreground">{{ t('storageCleanup.settings.last') }}</p>
      <p v-if="lastLoaded" :class="lastError && 'text-warning'">{{ lastError || lastText }}</p>
      <RouterLink to="/stats" class="text-2xs text-primary hover:underline">{{ t('storageCleanup.settings.openStats') }}</RouterLink>
    </div>
  </Card>
</template>
