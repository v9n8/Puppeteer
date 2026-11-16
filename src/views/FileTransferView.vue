<script setup lang="ts">
// File Transfer as a screen of the remote-desktop window (route `/file-transfer`,
// only there; lib/fileTransferWindow.ts says how it opens). The body is the one
// the own-window form uses (components/fileTransfer/FileTransferScreen.vue).
// "Open in new window" moves it to its own window and is locked while a copy is
// under way; leaving the screen mid-copy asks first, because it stops the queue.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { onBeforeRouteLeave } from 'vue-router'
import DetachButton from '@/components/app/DetachButton.vue'
import FileTransferScreen from '@/components/fileTransfer/FileTransferScreen.vue'
import { useConfirm } from '@/composables/useConfirm'
import { fileTransferTarget } from '@/lib/fileTransferWindow'
import { loadRemoteDesktopSession } from '@/lib/remoteDesktopSession'

const { t } = useI18n()
const { confirm } = useConfirm()

const device = loadRemoteDesktopSession()?.device || t('remote.desktop.banner.unnamed')
const screen = ref<InstanceType<typeof FileTransferScreen> | null>(null)
const lockedTip = computed(() => (screen.value?.busy ? t('fileTransfer.busyLocked') : undefined))

// The screen stops its queue itself when it unmounts (FileTransferScreen).
onBeforeRouteLeave(async () => {
  const n = screen.value?.activeCount ?? 0
  if (n === 0) return true
  return confirm({
    title: t('fileTransfer.leave.title'),
    message: t('fileTransfer.leave.message', { n }, n),
    confirmLabel: t('fileTransfer.leave.confirm'),
    cancelLabel: t('common.cancel'),
  })
})
</script>

<template>
  <div class="flex h-full min-h-0 flex-col gap-2 p-4">
    <header class="flex h-9 shrink-0 items-center gap-3">
      <h1 class="min-w-0 flex-1 truncate text-base font-semibold">{{ t('fileTransfer.screenTitle', { device }) }}</h1>
      <DetachButton :target="fileTransferTarget" :locked-tip="lockedTip" />
    </header>
    <FileTransferScreen ref="screen" class="min-h-0 flex-1" />
  </div>
</template>
