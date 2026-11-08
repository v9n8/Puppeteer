<script setup lang="ts">
// App update notice (main window, `app.auto_update`). Asks the backend whether a
// newer release exists at launch and then twice a day; installs only when the
// user agrees. Unmounted (timer cleared) while the prop is off, and the backend
// refuses to check then too.
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Download, X } from 'lucide-vue-next'
import { invoke } from '@/lib/tauri'
import { Button } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'

type UpdateCheck =
  | { status: 'unsupported' | 'up_to_date' }
  | { status: 'available'; version: string; current_version: string }

const CHECK_INTERVAL_MS = 12 * 60 * 60 * 1000

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()

const available = ref<string | null>(null)
const installing = ref(false)
// "Later" hides this version until the next launch; a newer one shows again.
let dismissed: string | null = null
let timer: number | null = null

async function check() {
  try {
    const res = await invoke<UpdateCheck>('check_update')
    if (res.status === 'unsupported') {
      if (timer) window.clearInterval(timer)
      timer = null
      return
    }
    available.value = res.status === 'available' && res.version !== dismissed ? res.version : null
  } catch (e) {
    // Offline, or the release has no manifest yet: try again at the next check.
    console.warn('check_update failed', e)
  }
}

function later() {
  dismissed = available.value
  available.value = null
}

async function install() {
  const version = available.value
  if (!version || installing.value) return
  const ok = await confirm({
    title: t('update.confirmTitle', { version }),
    message: t('update.confirmMessage'),
    confirmLabel: t('update.install'),
    variant: 'primary',
  })
  if (!ok) return
  installing.value = true
  try {
    const installed = await invoke<string>('install_update')
    available.value = null
    dismissed = installed
    toast.success(t('update.installed', { version: installed }), 8000)
  } catch (e) {
    toast.error(t('update.failed', { error: String(e) }))
  } finally {
    installing.value = false
  }
}

onMounted(() => {
  timer = window.setInterval(() => void check(), CHECK_INTERVAL_MS)
  void check()
})

onBeforeUnmount(() => {
  if (timer) window.clearInterval(timer)
})
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
