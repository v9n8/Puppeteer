<script setup lang="ts">
// App update notice (main window, `app.auto_update`). Starts the shared update
// check (composables/useUpdateCheck) at launch and then twice a day, and shows
// the corner toast when a newer release exists; installs only when the user
// agrees. Unmounted (timer cleared) while the prop is off, and the backend
// refuses to check then too.
import { onBeforeUnmount, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Download, X } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import { useUpdateCheck } from '@/composables/useUpdateCheck'

const { t } = useI18n()
const { toastVersion: available, installing, install, later, startAutoCheck } = useUpdateCheck()

let stop: (() => void) | null = null
onMounted(() => {
  stop = startAutoCheck()
})
onBeforeUnmount(() => stop?.())
</script>

<template>
  <div
    v-if="available"
    class="fixed bottom-4 right-4 z-[90] flex items-center gap-2 max-w-sm bg-card border border-border rounded-lg shadow-md px-3.5 py-2.5 text-xs"
    role="status"
  >
    <Download class="h-4 w-4 shrink-0 text-primary" :stroke-width="2" />
    <span class="flex-1 text-foreground leading-snug">{{ t('update.available', { version: available }) }}</span>
    <Button size="xs" :disabled="installing" @click="install">
      {{ installing ? t('update.installing') : t('update.install') }}
    </Button>
    <Button size="icon-xs" variant="ghost" :aria-label="t('update.later')" :title="t('update.later')" :disabled="installing" @click="later">
      <X class="h-3.5 w-3.5" :stroke-width="2" />
    </Button>
  </div>
</template>
