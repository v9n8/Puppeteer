<script setup lang="ts">
// THE Secret notes window (opened via lib/secretNotesWindow,
// `?secretNotesWindow=1[&projectId=][&id=]`): the shell around the shared body
// (components/secretNotes/SecretNotesPane.vue, which carries the vault UI and its
// rules). The shell owns what only a window has: the OS title, the pin, a re-point
// from the app, "Move back to main window", and the confirm before closing with an
// unsaved draft. The main window hosts the same body as the `/secret-notes` screen
// (views/SecretNotesView.vue).
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { ConfirmModal, ToastHost } from '@/components/ui'
import MergeBackButton from '@/components/app/MergeBackButton.vue'
import SecretNotesPane from '@/components/secretNotes/SecretNotesPane.vue'
import { useFloatingWindow } from '@/composables/useFloatingWindow'
import { refreshTray } from '@/lib/tray'
import { SECRET_NOTES_PATH, SECRET_NOTES_WINDOW_SELECT, type SecretNotesWindowSelect } from '@/lib/secretNotesWindow'

const { t } = useI18n()
const { pinned, togglePin } = useFloatingWindow()

const params = new URLSearchParams(window.location.search)
const pane = ref<InstanceType<typeof SecretNotesPane> | null>(null)

const windowTitle = computed(() => t('secretNotes.window.title'))
watch(
  windowTitle,
  async (value) => {
    document.title = value
    try {
      await getCurrentWindow().setTitle(value)
    } catch {
      /* outside the Tauri shell */
    }
    void refreshTray()
  },
  { immediate: true },
)

const unlisten: UnlistenFn[] = []
/** The close was confirmed: let the next close request through. */
let closing = false

/** This window closes on the move, so an unsaved draft is asked about first. */
async function confirmMerge(): Promise<boolean> {
  return (await pane.value?.confirmLeave()) ?? true
}

onMounted(async () => {
  try {
    unlisten.push(
      // Reopened from the app: re-point the project scope and / or select a note.
      await listen<SecretNotesWindowSelect>(SECRET_NOTES_WINDOW_SELECT, (e) => {
        if (e.payload) pane.value?.applySelect(e.payload)
      }),
      // Closing with an unsaved draft asks first.
      await getCurrentWindow().onCloseRequested(async (e) => {
        if (closing || !pane.value?.hasUnsaved()) return
        e.preventDefault()
        if (!(await pane.value.confirmDiscard())) return
        closing = true
        await getCurrentWindow().destroy()
      }),
    )
  } catch {
    /* running outside the Tauri shell */
  }
})

onBeforeUnmount(() => unlisten.forEach((off) => off()))
</script>

<template>
  <div class="flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground">
    <header class="flex h-10 shrink-0 items-center gap-2 border-b border-border/60 px-3">
      <h1 class="min-w-0 flex-1 truncate text-sm font-semibold">{{ t('secretNotes.window.title') }}</h1>
      <MergeBackButton screen="secret-notes" :path="SECRET_NOTES_PATH" :confirm-leave="confirmMerge" />
    </header>
    <SecretNotesPane
      ref="pane"
      mode="window"
      class="min-h-0 flex-1"
      :project-id="params.get('projectId')"
      :note-id="params.get('id')"
      :pinned="pinned"
      @toggle-pin="togglePin"
    />
    <!-- The pop-out doesn't mount the main app's dialog/toast hosts. -->
    <ConfirmModal />
    <ToastHost />
  </div>
</template>
