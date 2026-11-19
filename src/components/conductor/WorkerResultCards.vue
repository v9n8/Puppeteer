<script setup lang="ts">
// The workers that just finished a turn, as one result card each, in the
// conductor's conversation (V3-30). The numbers of edited files come from the
// same records and the same `changedFiles` the worker's "Changed files" tab uses,
// read when the card shows. The nudge the conductor got is one click away.
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronRight, Network } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import { changedFiles } from '@/lib/editStats'
import { runStatusColorClass } from '@/lib/runDisplay'
import { parseStreamLogWindow } from '@/lib/streamEvents'
import { shortWorkerId, workerResultLine, type WorkerEdits, type WorkerResult } from '@/lib/workerResults'
import { useRunsStore } from '@/stores/runs'
import { useSessionTabsStore } from '@/stores/sessionTabs'
import { useRunSessionOptional } from '@/components/sessions/useRunSession'

const props = defineProps<{
  workers: WorkerResult[]
  /** The message the conductor received after these cards. */
  nudge?: string
}>()

const { t } = useI18n()
const runsStore = useRunsStore()
const tabsStore = useSessionTabsStore()
// Null outside RunView (the phone controller): the cards still show, without buttons.
const session = useRunSessionOptional()

// Per worker and finish time, so scrolling the log does not read the same file twice.
const editsCache = new Map<string, WorkerEdits>()
const edits = ref<Record<string, WorkerEdits>>({})
const showNudge = ref(false)

async function loadEdits(w: WorkerResult) {
  const key = `${w.id}@${w.finished_at}`
  let found = editsCache.get(key)
  if (!found) {
    try {
      const r = await runsStore.getRunEditRecords(w.id)
      const files = changedFiles(parseStreamLogWindow(r.records).entries, new Set(r.truncated))
      found = {
        files: files.length,
        added: files.reduce((n, f) => n + f.added, 0),
        removed: files.reduce((n, f) => n + f.removed, 0),
      }
      editsCache.set(key, found)
    } catch (e) {
      // The card still shows, without the "Edited N files" part.
      console.warn(`worker result card: could not read the edit records of worker ${w.id}`, e)
      return
    }
  }
  edits.value = { ...edits.value, [w.id]: found }
}

onMounted(() => { props.workers.forEach((w) => { void loadEdits(w) }) })

// The conductor's worker list leaves out trashed workers; until it has loaded, nothing is locked.
function inTrash(id: string): boolean {
  const list = session?.conductorWorkers.value ?? []
  return list.length > 0 && !list.some((w) => w.worker_id === id)
}

const cards = computed(() => props.workers.map((w) => {
  const e = edits.value[w.id] ?? null
  return {
    w,
    line: workerResultLine(w, e, t),
    canSeeFiles: !!e && e.files > 0,
    trashed: inTrash(w.id),
    statusLabel: t(`conductor.result.status.${w.status === 'failed' || w.status === 'cancelled' ? w.status : 'done'}`),
  }
}))

function openWorker(id: string) {
  void session?.openSession(id)
}

function openChanged(id: string) {
  tabsStore.openChanged(id)
  void session?.openSession(id)
}
</script>

<template>
  <div class="my-1.5 flex flex-col gap-2">
    <div
      v-for="c in cards"
      :key="c.w.id"
      class="rounded-[10px] border border-border bg-background px-3 py-2.5 text-[13px]"
    >
      <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
        <Network class="h-4 w-4 shrink-0 text-foreground/60" :stroke-width="1.75" />
        <span class="font-medium text-foreground">{{ t('conductor.result.title') }}</span>
        <span class="min-w-0 break-words text-foreground/70">
          <span class="font-mono">{{ shortWorkerId(c.w.id) }}</span><template v-if="c.w.title"> · {{ c.w.title }}</template>
        </span>
        <span class="ml-auto shrink-0 font-medium" :class="runStatusColorClass(c.w.status === 'failed' || c.w.status === 'cancelled' ? c.w.status : 'done')">
          {{ c.statusLabel }}
        </span>
      </div>
      <div v-if="c.line" class="mt-1 min-w-0 text-foreground/60 [overflow-wrap:anywhere]">{{ c.line }}</div>
      <div v-if="session" class="mt-2 flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          variant="outline"
          class="min-h-7"
          :disabled="c.trashed"
          :title="c.trashed ? t('conductor.result.inTrash') : undefined"
          @click="openWorker(c.w.id)"
        >{{ t('conductor.result.openWorker') }} →</Button>
        <Button
          size="sm"
          variant="outline"
          class="min-h-7"
          :disabled="c.trashed || !c.canSeeFiles"
          :title="c.trashed ? t('conductor.result.inTrash') : undefined"
          @click="openChanged(c.w.id)"
        >{{ t('conductor.result.openChanged') }}</Button>
      </div>
    </div>
    <div v-if="nudge" class="text-xs text-foreground/55">
      <button
        class="inline-flex min-h-7 items-center gap-1 hover:text-foreground/80 focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
        @click="showNudge = !showNudge"
      >
        <ChevronRight class="h-3 w-3 transition-transform" :class="{ 'rotate-90': showNudge }" :stroke-width="2" />
        {{ t('conductor.result.nudge') }}
      </button>
      <div v-if="showNudge" class="mt-1 min-w-0 [overflow-wrap:anywhere] whitespace-pre-wrap rounded-lg border border-border bg-muted/40 px-3 py-2.5 leading-relaxed">{{ nudge }}</div>
    </div>
  </div>
</template>
