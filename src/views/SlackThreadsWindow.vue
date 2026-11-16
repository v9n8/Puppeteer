<script setup lang="ts">
// THE Slack window (opened via lib/slackWindow, `?slackWindow=1[&id=][&add=1]
// [&projectId=]`): the shell around the shared body (components/slack/
// SlackThreadsPane.vue, the reader itself). The shell owns what only a window has:
// the OS title (the menu-bar switcher's row), the URL that follows the selection,
// asking the main window to show an app screen, and "Move back to main window".
// The main window hosts the same body as the `/slack` screen (views/SlackView.vue).
import { computed, ref, watch } from 'vue'
import { emit } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { ConfirmModal, ToastHost } from '@/components/ui'
import MergeBackButton from '@/components/app/MergeBackButton.vue'
import SlackThreadsPane from '@/components/slack/SlackThreadsPane.vue'
import { refreshTray } from '@/lib/tray'
import { SLACK_WINDOW_OPEN_ROUTE, type SlackWindowOpenRoute } from '@/lib/slackWindow'
import { slackPath } from '@/lib/screenRoutes'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
const params = new URLSearchParams(window.location.search)
const initialId = params.get('id') || null
const initialProject = params.get('projectId') || null

const pane = ref<InstanceType<typeof SlackThreadsPane> | null>(null)
const selectedId = ref<string | null>(initialId)
/** Where "Move back to main window" lands: the thread being read. */
const mergePath = computed(() => slackPath({ id: selectedId.value }))

// The URL follows the selection, so a reload never reopens a dead id.
function onSelected(id: string | null) {
  selectedId.value = id
  const url = new URL(window.location.href)
  if (id) url.searchParams.set('id', id)
  else url.searchParams.delete('id')
  url.searchParams.delete('add')
  window.history.replaceState(null, '', url.toString())
}

watch(
  () => pane.value?.windowTitle ?? t('slackThread.window.title'),
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

/** This window has no router: the main window shows the screen (e.g. the MCP settings when there is no token). */
function openRoute(path: string) {
  emit(SLACK_WINDOW_OPEN_ROUTE, { path } satisfies SlackWindowOpenRoute).catch(() => {})
}
</script>

<template>
  <div class="flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground">
    <header class="flex h-10 shrink-0 items-center gap-2 border-b border-border/60 px-3">
      <h1 class="min-w-0 flex-1 truncate text-sm font-semibold">{{ t('slackThread.nav') }}</h1>
      <MergeBackButton screen="slack" :path="mergePath" />
    </header>
    <SlackThreadsPane
      ref="pane"
      standalone
      class="min-h-0 flex-1"
      :id="initialId"
      :project-id="initialProject"
      :add="params.get('add') === '1'"
      @selected="onSelected"
      @open-route="openRoute"
    />
    <!-- The pop-out doesn't mount the main app's dialog/toast hosts. -->
    <ConfirmModal />
    <ToastHost />
  </div>
</template>
