<script setup lang="ts">
// Slack as a screen of the main window (`/slack[?project=][&id=][&add=1]`), or of
// its own window (views/SlackThreadsWindow.vue): lib/slackWindow keeps where the
// user left it, DetachButton / MergeBackButton move it. The reader is
// components/slack/SlackThreadsPane.vue, the same body the window shows. The
// address follows the selected thread, so Open in new window carries it along.
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { PageHeader } from '@/components/ui'
import DetachButton from '@/components/app/DetachButton.vue'
import SlackThreadsPane from '@/components/slack/SlackThreadsPane.vue'
import { openSlackThreadsWindow, slackTarget } from '@/lib/slackWindow'
import { pageCount } from '@/lib/pageCount'
import { useSlackThreadsStore } from '@/stores/slackThreads'
import { SLACK_PATH, parseSlackQuery, slackPath } from '@/lib/screenRoutes'

const { t } = useI18n()
const store = useSlackThreadsStore()
const route = useRoute()
const router = useRouter()

const initial = parseSlackQuery(route.query)
const pane = ref<InstanceType<typeof SlackThreadsPane> | null>(null)
const selectedId = ref<string | null>(initial.id)

// Asked again (sidebar, ⌘K, a linked thread, a notification) while the screen is open: re-point it.
watch(
  () => route.fullPath,
  () => {
    if (route.path !== SLACK_PATH) return
    const { projectId, id, add } = parseSlackQuery(route.query)
    pane.value?.applyLocation({ id, projectId })
    if (add) pane.value?.openAdd(projectId)
  },
)

// The address follows the selection, so a reload or a copied link reopens it.
function onSelected(id: string | null) {
  selectedId.value = id
  syncUrl()
}
function syncUrl() {
  if (route.path !== SLACK_PATH) return
  const target = slackPath({ id: selectedId.value })
  if (target === route.fullPath) return
  router.replace(target).catch(() => {})
}
onMounted(syncUrl)

const detachTarget = computed(() => ({
  ...slackTarget,
  openWindow: () => openSlackThreadsWindow({ id: selectedId.value ?? undefined }),
}))
</script>

<template>
  <div class="flex h-full min-h-0 flex-col bg-background">
    <PageHeader
      :title="t('slackThread.nav')"
      :count="pageCount(store.loaded, store.error, store.threads.length)"
    >
      <template #actions>
        <DetachButton :target="detachTarget" />
      </template>
    </PageHeader>
    <SlackThreadsPane
      ref="pane"
      class="min-h-0 flex-1"
      :id="initial.id"
      :project-id="initial.projectId"
      :add="initial.add"
      @selected="onSelected"
      @open-route="router.push($event).catch(() => {})"
    />
  </div>
</template>
