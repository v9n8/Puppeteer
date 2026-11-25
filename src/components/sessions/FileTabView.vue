<script setup lang="ts">
// The file tabs of the Session area (V3-24): a FileViewer per open file tab, the
// selected one shown. A tab's viewer stays mounted once it was shown, so going
// back to it keeps the scroll and an edit in progress; closing the tab drops it.
// "Open in new window" moves the file to its own window and closes the tab.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { openUrl } from '@tauri-apps/plugin-opener'
import { SquareArrowOutUpRight } from 'lucide-vue-next'
import FileViewer from '@/components/FileViewer.vue'
import { Button } from '@/components/ui'
import { FILE_DELETED_EVENT, FILE_MOVED_EVENT } from '@/lib/fileEvents'
import { openFileWindow } from '@/lib/fileWindow'
import { treePathIn } from '@/lib/sessionPaths'
import { sendQuote } from '@/lib/quoteAppend'
import type { QuoteSource } from '@/lib/quote'
import type { FileTab } from '@/lib/sessionTabs'
import { useSessionTabsStore } from '@/stores/sessionTabs'
import { useToast } from '@/composables/useToast'
import { useRunSession } from './useRunSession'

// `activeId`: the tab this area shows (the right side's while split, V3-27).
const props = defineProps<{ activeId: string }>()
const { t } = useI18n()
const { toast } = useToast()
const { currentRunId, currentRun, project, runLabel, sessionRoot, sessionRootSettled } = useRunSession()
const tabsStore = useSessionTabsStore()

const runId = computed(() => currentRunId.value ?? '')
const projectPath = computed(() => project.value?.path ?? '')
const state = computed(() => tabsStore.stateFor(runId.value))
// Tabs saved under two spellings of one file merge (RT-13), once the root is settled.
watch([runId, sessionRoot, sessionRootSettled], ([id, root, settled]) => {
  if (id && root && settled) tabsStore.normalize(id, root)
}, { immediate: true })
const fileTabs = computed(() => state.value.tabs.filter((tab): tab is FileTab => tab.kind === 'file'))

// Tabs shown at least once in this Session; the others mount on first view.
const shown = ref(new Set<string>())
watch(runId, () => { shown.value = new Set() })
watch(() => props.activeId, (id) => {
  if (id.startsWith('file:') && !shown.value.has(id)) shown.value = new Set([...shown.value, id])
}, { immediate: true })
const mounted = computed(() => fileTabs.value.filter((tab) => shown.value.has(tab.id)))

// Quote goes to this Session's composer, through the same route as a file window's,
// and focuses it: the composer is in this window (RT-19).
const quoteTarget = computed(() => {
  const run = currentRun.value
  if (!run) return null
  return {
    label: runLabel(run),
    append: (text: string, source: QuoteSource, note?: string) => {
      sendQuote(run.id, text, source, note, true).catch((e) => toast.error(String(e)))
    },
  }
})

// The Files tree renames and deletes in another component (or webview): the tabs
// follow, as a file window does. The tree lists the Project: a worker's tabs of
// its worktree copies are other files and stay.
function inRoot(treeProject: string, path: string): string | null {
  return projectPath.value ? treePathIn(treeProject, path, sessionRoot.value, projectPath.value) : null
}
let unlisten: UnlistenFn[] = []
let gone = false
onMounted(async () => {
  try {
    const handlers = [
      await listen<{ projectPath: string; path: string }>(FILE_DELETED_EVENT, ({ payload: p }) => {
        const path = p && inRoot(p.projectPath, p.path)
        if (path && runId.value) tabsStore.removeUnder(runId.value, path)
      }),
      await listen<{ projectPath: string; from: string; to: string }>(FILE_MOVED_EVENT, ({ payload: p }) => {
        const from = p && inRoot(p.projectPath, p.from)
        const to = p && inRoot(p.projectPath, p.to)
        if (from && to && runId.value) tabsStore.rename(runId.value, from, to)
      }),
    ]
    if (gone) handlers.forEach((u) => u())
    else unlisten = handlers
  } catch {
    /* running outside the Tauri shell */
  }
})
onBeforeUnmount(() => {
  gone = true
  unlisten.forEach((u) => u())
  unlisten = []
})

function onOpenFile(path: string, line: number | null) {
  tabsStore.open(runId.value, path, line, sessionRoot.value)
}
function onOpenUrl(url: string) {
  openUrl(url).catch((e) => toast.error(String(e)))
}

async function detach(tab: FileTab) {
  const id = runId.value
  try {
    await openFileWindow(projectPath.value, tab.path, tab.line, id, sessionRoot.value)
    tabsStore.close(id, tab.id)
  } catch (e) {
    toast.error(String(e))
  }
}
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col overflow-hidden bg-card">
    <FileViewer
      v-for="tab in mounted"
      v-show="activeId === tab.id"
      :key="`${runId}|${tab.id}`"
      class="flex-1"
      :project-path="sessionRoot"
      :path="tab.path"
      :line="tab.line"
      :can-compare="true"
      :quote-target="quoteTarget"
      @open-file="onOpenFile"
      @open-url="onOpenUrl"
    >
      <template #actions>
        <Button
          variant="ghost"
          size="icon-sm"
          class="shrink-0"
          :title="t('run.sessionTabs.detachTip')"
          :aria-label="t('run.sessionTabs.detachTip')"
          @click="detach(tab)"
        >
          <SquareArrowOutUpRight class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
        </Button>
      </template>
    </FileViewer>
  </div>
</template>
