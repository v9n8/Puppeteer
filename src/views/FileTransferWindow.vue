<script setup lang="ts">
// THE File Transfer window (opened via lib/fileTransferWindow, `?fileTransferWindow=1`,
// remote set only): the title bar of the window around the shared body
// (components/fileTransfer/FileTransferScreen.vue). "Move back to main window"
// brings File Transfer into the remote-desktop window instead; it is locked while
// a copy is under way, because moving rebuilds the screen and stops the queue.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { ConfirmModal, PromptModal, ToastHost } from '@/components/ui'
import MergeBackButton from '@/components/app/MergeBackButton.vue'
import FileTransferScreen from '@/components/fileTransfer/FileTransferScreen.vue'
import { refreshTray } from '@/lib/tray'
import { FILE_TRANSFER_PATH } from '@/lib/fileTransferWindow'
import { loadRemoteDesktopSession } from '@/lib/remoteDesktopSession'

const { t, locale } = useI18n()

const device = loadRemoteDesktopSession()?.device || t('remote.desktop.banner.unnamed')
const screen = ref<InstanceType<typeof FileTransferScreen> | null>(null)
const lockedTip = computed(() => (screen.value?.busy ? t('fileTransfer.busyLocked') : undefined))

watch(
  locale,
  async () => {
    const title = t('fileTransfer.windowTitle', { device })
    document.title = title
    try {
      await getCurrentWindow().setTitle(title)
    } catch {
      /* outside the Tauri shell */
    }
    void refreshTray()
  },
  { immediate: true },
)
</script>

<template>
  <div class="flex h-screen w-screen flex-col gap-2 overflow-hidden bg-background p-3 text-foreground">
    <header class="flex shrink-0 items-center gap-3 px-1">
      <h1 class="min-w-0 flex-1 truncate text-sm font-semibold">{{ t('fileTransfer.title') }}</h1>
      <MergeBackButton
        screen="file-transfer"
        :path="FILE_TRANSFER_PATH"
        :tip="t('fileTransfer.mergeTip')"
        :locked-tip="lockedTip"
      />
    </header>
    <FileTransferScreen ref="screen" class="min-h-0 flex-1" />
    <ConfirmModal />
    <PromptModal />
    <ToastHost />
  </div>
</template>
