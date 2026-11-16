<script setup lang="ts">
// Every window that is not the main one: the bare pop-outs (file viewer,
// permission prompt, todo / note, lists, Secret notes, Slack, PR Inbox,
// Calendar, Focus, File Transfer, Gantt, Settings) and the session pop-out. App.vue
// renders this instead of the main window's chrome.
import { computed } from 'vue'
import { RouterView } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { PowerOff } from 'lucide-vue-next'
import { useFeaturesStore } from '@/stores/features'
import FileViewerWindow from '@/views/FileViewerWindow.vue'
import PermissionWindow from '@/views/PermissionWindow.vue'
import { ConfirmModal, PromptModal, ToastHost } from '@/components/ui'
import ImageCompareHost from '@/components/ImageCompareHost.vue'
import WindowsOverlay from '@/components/windows/WindowsOverlay.vue'
import ItemWindow from '@/views/ItemWindow.vue'
import ItemListWindow from '@/views/ItemListWindow.vue'
import SecretNotesWindow from '@/views/SecretNotesWindow.vue'
import SlackThreadsWindow from '@/views/SlackThreadsWindow.vue'
import PrInboxWindow from '@/views/PrInboxWindow.vue'
import CalendarWindow from '@/views/CalendarWindow.vue'
import FocusWindow from '@/views/FocusWindow.vue'
import FileTransferWindow from '@/views/FileTransferWindow.vue'
import SettingsWindow from '@/views/SettingsWindow.vue'
import IssuesGanttView from '@/views/IssuesGanttView.vue'
import { popoutFeature } from '@/lib/featureGate'
import {
  IS_CALENDAR_WINDOW as isCalendarWindow,
  IS_FILE_TRANSFER_WINDOW as isFileTransferWindow,
  IS_FILE_WINDOW as isFileWindow,
  IS_FOCUS_WINDOW as isFocusWindow,
  IS_LEGACY_TODO_LIST_WINDOW as isLegacyTodoListWindow,
  IS_GANTT_WINDOW as isGanttWindow,
  IS_ITEM_LIST_WINDOW as isItemListWindow,
  IS_ITEM_WINDOW as isItemWindow,
  IS_PERMISSION_WINDOW as isPermissionWindow,
  IS_PR_INBOX_WINDOW as isPrInboxWindow,
  IS_SECRET_NOTES_WINDOW as isSecretNotesWindow,
  IS_SESSION_WINDOW as isSessionWindow,
  IS_SETTINGS_WINDOW as isSettingsWindow,
  IS_SLACK_WINDOW as isSlackWindow,
} from '@/lib/windowMode'


// The feature a gated pop-out (Focus, PR Inbox, Todos, …) belongs to, if this is one.
const gatedPopout = popoutFeature(window.location.search)

const features = useFeaturesStore()
const gatedPopoutOn = computed(() => !!gatedPopout && features.loaded && features.isOn(gatedPopout))
const { t } = useI18n()
</script>

<template>
  <!-- A gated pop-out whose feature is off (or, blank, not loaded yet): a notice
       instead of the window, so one left open when the switch flipped stops. -->
  <div
    v-if="gatedPopout && !gatedPopoutOn"
    class="app-shell flex h-screen w-screen items-center justify-center bg-background p-6 text-foreground"
  >
    <div v-if="features.loaded" class="flex max-w-sm flex-col items-center gap-3 text-center">
      <div class="flex h-12 w-12 items-center justify-center rounded-xl bg-muted">
        <PowerOff class="h-6 w-6 text-muted-foreground" :stroke-width="1.5" />
      </div>
      <p class="text-sm font-semibold">{{ t('features.disabled.title') }}</p>
      <p class="text-xs text-muted-foreground">{{ t('features.disabled.body') }}</p>
    </div>
  </div>

  <!-- Pop-out file viewer window: bare layout, no app chrome. -->
  <FileViewerWindow v-else-if="isFileWindow" />

  <!-- Pop-out permission prompt window: bare layout, mirrors the main window. -->
  <PermissionWindow v-else-if="isPermissionWindow" />

  <!-- THE todo / note window: create or edit, the only place either is written. -->
  <ItemWindow v-else-if="isItemWindow" />

  <!-- THE todo / note LIST window: stands beside the main window, replacing the drawer. -->
  <ItemListWindow v-else-if="isItemListWindow && !isLegacyTodoListWindow" />

  <!-- THE Secret notes window: the password-sealed vault (local only). -->
  <SecretNotesWindow v-else-if="isSecretNotesWindow" />

  <!-- THE Slack threads window: Slack-like reader of the fetched + translated threads. -->
  <SlackThreadsWindow v-else-if="isSlackWindow" />

  <!-- THE PR Inbox window: the PR Inbox screen on its own OS window. -->
  <PrInboxWindow v-else-if="isPrInboxWindow" />

  <!-- THE Calendar window: the Calendar screen on its own OS window. -->
  <CalendarWindow v-else-if="isCalendarWindow" />

  <!-- THE Tasks window: the Tasks screen on its own OS window (also what an old Todos window loads). -->
  <FocusWindow v-else-if="isFocusWindow || isLegacyTodoListWindow" />

  <!-- THE File Transfer window (remote desktop only): this machine ⇄ the controlled Puppeteer. -->
  <FileTransferWindow v-else-if="isFileTransferWindow" />

  <!-- THE Settings window: the Settings screen on its own OS window (when the user moved it there). -->
  <SettingsWindow v-else-if="isSettingsWindow" />

  <!-- Pop-out Gantt window: bare Gantt chart on its own OS window. -->
  <!-- Gantt pop-out: the board plus the dialog / toast hosts its row menu
       ("+ Todo") needs, like PrInboxWindow. -->
  <div
    v-else-if="isGanttWindow"
    class="app-shell flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground"
  >
    <IssuesGanttView class="min-h-0 flex-1" />
    <ConfirmModal />
    <ToastHost />
  </div>

  <!-- Session pop-out: one run in its own OS window. Reuses the full RunView via
       the router, but with no sidebar / nav / docks / app-wide notifiers — just
       the conversation, its toolbar and composer. -->
  <div
    v-else-if="isSessionWindow"
    class="app-shell flex flex-col h-screen bg-background text-foreground overflow-hidden"
  >
    <main class="flex-1 min-w-0 flex flex-col overflow-hidden">
      <div class="flex-1 min-w-0 overflow-auto">
        <RouterView />
      </div>
    </main>
    <!-- Overlay hosts the run screen relies on; NOT PermissionNotifier /
         CalendarReminder — those stay owned by the main window. Settings opens
         in the main window or its own, never here (router settingsGuard). -->
    <ConfirmModal />
    <PromptModal />
    <ToastHost />
    <ImageCompareHost />
  </div>

  <!-- "Open windows" over whichever window has the focus (⌘⇧E); not over the
       transient permission prompt. -->
  <WindowsOverlay v-if="!isPermissionWindow" />
</template>
