<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { FileDiff } from 'lucide-vue-next'
import type { StreamEntry } from '@/lib/streamEvents'
import { isWriteTool } from '@/lib/editStats'
import { useSessionTabsStore } from '@/stores/sessionTabs'
// Path extraction lives in `@/lib/toolFiles` so this count and the run log's own
// file links always agree on what a tool call touched.
import { toolFileTargets } from '@/lib/toolFiles'
import { useRunSession } from '@/components/sessions/useRunSession'

const { t } = useI18n()
const { currentRunId } = useRunSession()
const tabsStore = useSessionTabsStore()

const props = defineProps<{
  entries: StreamEntry[]
}>()

// How many files the Session wrote or created. Read-only tools (Read, Grep, Glob,
// NotebookRead…) are deliberately excluded: this answers "what did this session
// write", not "what did it look at". The list itself is the Changed files tab.
const count = computed(() => {
  const paths = new Set<string>()
  for (const e of props.entries) {
    if (e.kind !== 'tool' || !isWriteTool(e.name)) continue
    // A rejected or failed write never reached the disk. A still-running call
    // has no result yet, so it stays counted (optimistic while streaming).
    if (e.result?.is_error) continue
    for (const path of toolFileTargets(e.input)) paths.add(path)
  }
  return paths.size
})

function openTab() {
  if (currentRunId.value) tabsStore.openChanged(currentRunId.value)
}
</script>

<template>
  <button
    v-if="count"
    type="button"
    class="flex h-6 cursor-pointer items-center gap-1.5 rounded-md px-2 text-2xs font-medium leading-none text-foreground/50 transition-colors hover:bg-card hover:text-foreground/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    :title="t('misc.mentionedFiles.countTitle', { count })"
    @click="openTab"
  >
    <FileDiff class="h-3.5 w-3.5" :stroke-width="1.75" />
    <span>{{ t('misc.mentionedFiles.button', { count }) }}</span>
  </button>
</template>
