<script setup lang="ts">
// Trash: sessions and projects deleted from History / Projects, grouped by
// project (lib/trash). Restore brings them back; a permanent delete (purge)
// removes logs, transcripts and links for good. Purging is local-only, so a
// remote desktop sees restore buttons only.
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { RotateCw, Trash2 } from 'lucide-vue-next'
import { Banner, Button, PageHeader } from '@/components/ui'
import TrashGroupCard from '@/components/trash/TrashGroupCard.vue'
import TrashToolbar from '@/components/trash/TrashToolbar.vue'
import { invoke } from '@/lib/tauri'
import { isRemoteMode } from '@/lib/ipc/remote'
import {
  CMD_LIST_TRASH,
  EVT_TRASH_CHANGED,
  filterTrash,
  groupActionTargets,
  groupTrash,
  initialOpenGroups,
  parseTrashSort,
  sessionsLost,
  trashTotals,
  type TrashEntry,
  type TrashListing,
  type TrashRun,
  type TrashSession,
  type TrashSort,
} from '@/lib/trash'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'

defineOptions({ name: 'TrashView' })

const { t } = useI18n()
const router = useRouter()
const { confirm } = useConfirm()
const { toast } = useToast()
const remote = isRemoteMode()

const listing = shallowRef<TrashListing>({ projects: [], runs: [] })
const loading = ref(false)
const error = ref<string | null>(null)
/** An action is running: every button waits for it. */
const busy = ref(false)

// The sort choice is a per-viewer convenience, kept in this browser only.
const SORT_KEY = 'puppeteer.trashSort'
function loadSort(): TrashSort {
  try {
    return parseTrashSort(localStorage.getItem(SORT_KEY))
  } catch {
    return 'deleted-desc'
  }
}
const sort = ref<TrashSort>(loadSort())
function setSort(value: TrashSort) {
  sort.value = value
  try {
    localStorage.setItem(SORT_KEY, value)
  } catch {
    /* storage unavailable: the choice lasts for this view only */
  }
}

const groups = computed(() => groupTrash(listing.value, sort.value))
const totals = computed(() => trashTotals(groups.value))

const query = ref('')
const searching = computed(() => query.value.trim() !== '')
const entries = computed(() => filterTrash(groups.value, query.value, runLabel))
const matchedSessions = computed(() => entries.value.reduce((n, e) => n + e.group.sessions.length, 0))

// Which blocks are open is a per-viewer convenience too: by project id, the
// first block open until the viewer chooses.
const OPEN_KEY = 'puppeteer.trashOpenGroups'
function loadOpen(): Record<string, boolean> | null {
  try {
    const raw = JSON.parse(localStorage.getItem(OPEN_KEY) ?? 'null')
    return raw && typeof raw === 'object' ? raw : null
  } catch {
    return null
  }
}
const storedOpen = ref<Record<string, boolean> | null>(loadOpen())
const openIds = computed(() => initialOpenGroups(groups.value, storedOpen.value))
/** While searching every block with a result is open; the chevron can still fold one until the text changes. */
const foldedWhileSearching = ref(new Set<string>())
watch(query, () => (foldedWhileSearching.value = new Set()))
function isOpen(id: string): boolean {
  return searching.value ? !foldedWhileSearching.value.has(id) : openIds.value.has(id)
}
function toggleGroup(id: string) {
  if (searching.value) {
    const next = new Set(foldedWhileSearching.value)
    if (!next.delete(id)) next.add(id)
    foldedWhileSearching.value = next
    return
  }
  storedOpen.value = { ...storedOpen.value, [id]: !openIds.value.has(id) }
  try {
    localStorage.setItem(OPEN_KEY, JSON.stringify(storedOpen.value))
  } catch {
    /* storage unavailable: the choice lasts for this view only */
  }
}

async function load() {
  loading.value = true
  try {
    listing.value = await invoke<TrashListing>(CMD_LIST_TRASH)
    error.value = null
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

let unlisten: UnlistenFn | null = null
let disposed = false
onMounted(async () => {
  void load()
  try {
    const un = await listen(EVT_TRASH_CHANGED, () => void load())
    if (disposed) un()
    else unlisten = un
  } catch {
    // Outside the Tauri shell (browser dev): no live refresh.
  }
})
onBeforeUnmount(() => {
  disposed = true
  unlisten?.()
})

function runLabel(run: TrashRun): string {
  if (run.title?.trim()) return run.title.trim()
  if (run.run_type === 'session') return t('run.labelSession')
  return `${run.run_type === 'analyze_issue' ? t('run.labelIssue') : t('run.labelPr')} #${run.ref_number}`
}

function sessionCount(count: number): string {
  return t('trash.sessionCount', { count }, count)
}

/** Runs one action, then reloads; errors become a toast. */
async function act(work: () => Promise<void>) {
  if (busy.value) return
  busy.value = true
  try {
    await work()
  } catch (e) {
    toast.error(String(e))
  } finally {
    busy.value = false
    void load()
  }
}

/** Purges each run, collecting the transcripts the backend could not remove. */
async function purgeRuns(runIds: string[]): Promise<string[]> {
  const leftover: string[] = []
  for (const runId of runIds) leftover.push(...(await invoke<string[]>('purge_run', { runId })))
  return leftover
}

function toastPurged(leftover: string[], done: string) {
  if (leftover.length) {
    toast.error(t('trash.toast.purgedWithLeftovers', { count: leftover.length, paths: leftover.join('\n') }))
  } else {
    toast.success(done)
  }
}

function restoreSession(group: TrashEntry['full'], session: TrashSession) {
  void act(async () => {
    if (group.projectTrashed && !(await confirm({
      title: t('trash.confirm.restoreWithProjectTitle'),
      message: t('trash.confirm.restoreWithProjectMessage', { name: group.project.name }),
      confirmLabel: t('trash.restore'),
      variant: 'primary',
    }))) return
    const outcome = await invoke<{ run_ids: string[]; project_restored: boolean }>('restore_run', { runId: session.run.id })
    toast.success(outcome.project_restored ? t('trash.toast.restoredWithProject', { name: group.project.name }) : t('trash.toast.restored'))
  })
}

function purgeSession(session: TrashSession) {
  void act(async () => {
    const count = sessionsLost(session.run)
    if (!(await confirm({
      title: t('trash.confirm.purgeTitle'),
      message: t('trash.confirm.purgeSessionMessage', { label: runLabel(session.run), count }, count),
      confirmLabel: t('trash.purge'),
    }))) return
    toastPurged(await purgeRuns([session.run.id]), t('trash.toast.purged'))
  })
}

function restoreGroup(entry: TrashEntry) {
  void act(async () => {
    const group = entry.full
    const targets = groupActionTargets(group, entry.matched)
    if (targets.wholeProject) {
      await invoke('restore_project', { id: group.project.id })
      toast.success(t('trash.toast.projectRestored', { name: group.project.name }))
      return
    }
    // Restoring a session of a trashed project restores the project too (restore_run).
    if (targets.partial && group.projectTrashed && !(await confirm({
      title: t('trash.confirm.restoreWithProjectTitle'),
      message: t('trash.confirm.restoreMatchedMessage', {
        count: targets.sessions.length,
        name: group.project.name,
        others: targets.restoredWithProject,
      }, targets.sessions.length),
      confirmLabel: t('trash.restore'),
      variant: 'primary',
    }))) return
    for (const s of targets.sessions) await invoke('restore_run', { runId: s.run.id })
    toast.success(t('trash.toast.groupRestored', { count: targets.sessions.length }, targets.sessions.length))
  })
}

function purgeGroup(entry: TrashEntry) {
  void act(async () => {
    const group = entry.full
    const targets = groupActionTargets(group, entry.matched)
    const count = targets.purgeCount
    const name = group.project.name
    const message = targets.partial
      ? t('trash.confirm.purgeMatchedMessage', {
          count: targets.sessions.length,
          query: query.value.trim(),
          name,
          remaining: targets.remaining,
        }, targets.sessions.length)
      : group.projectTrashed
        ? t('trash.confirm.purgeProjectMessage', { name, count }, count)
        : t('trash.confirm.purgeGroupMessage', { name, count }, count)
    if (!(await confirm({
      title: t('trash.confirm.purgeTitle'),
      message,
      confirmLabel: t('trash.purge'),
    }))) return
    const leftover = targets.wholeProject
      ? await invoke<string[]>('purge_project', { id: group.project.id })
      : await purgeRuns(targets.sessions.map((s) => s.run.id))
    toastPurged(leftover, t('trash.toast.purged'))
  })
}

function emptyTrash() {
  void act(async () => {
    const { projects, sessions } = totals.value
    if (!(await confirm({
      title: t('trash.emptyTrash'),
      message: t('trash.confirm.emptyMessage', {
        sessions: sessionCount(sessions),
        projects: t('trash.projectCount', { count: projects }, projects),
      }),
      confirmLabel: t('trash.emptyTrash'),
    }))) return
    const leftover: string[] = []
    for (const group of groups.value) {
      if (group.projectTrashed) leftover.push(...(await invoke<string[]>('purge_project', { id: group.project.id })))
      else leftover.push(...(await purgeRuns(group.sessions.map((s) => s.run.id))))
    }
    toastPurged(leftover, t('trash.toast.emptied'))
  })
}

function openProject(group: TrashEntry['full']) {
  if (!group.projectTrashed) void router.push(`/projects/${group.project.id}`)
}
</script>

<template>
  <div class="flex flex-col h-full">
    <PageHeader :title="t('trash.title')" :count="totals.sessions || null" :count-title="sessionCount(totals.sessions)">
      <template #actions>
        <Button variant="ghost" size="xs" :disabled="loading" @click="load">
          <RotateCw class="h-3.5 w-3.5" :class="loading && 'animate-spin'" :stroke-width="2" />
          {{ t('trash.refresh') }}
        </Button>
        <Button v-if="!remote" variant="destructive" size="xs" :disabled="busy || groups.length === 0" @click="emptyTrash">
          <Trash2 class="h-3.5 w-3.5" :stroke-width="2" />
          {{ t('trash.emptyTrash') }}
        </Button>
      </template>
    </PageHeader>
    <TrashToolbar v-model:query="query" :sort="sort" @update:sort="setSort" />

    <div class="flex-1 overflow-auto p-6">
      <div
        v-if="!loading && !error && groups.length === 0"
        class="flex flex-col items-center justify-center h-full min-h-80 text-center"
      >
        <div class="flex h-12 w-12 items-center justify-center rounded-xl bg-muted mb-4">
          <Trash2 class="h-6 w-6 text-muted-foreground" :stroke-width="1.5" />
        </div>
        <p class="text-sm font-medium">{{ t('trash.empty.title') }}</p>
        <p class="text-xs text-muted-foreground mt-1 max-w-80">{{ t('trash.empty.hint') }}</p>
      </div>

      <div v-else class="max-w-4xl space-y-4">
        <p class="text-xs text-muted-foreground">{{ t('trash.hint') }}</p>
        <Banner v-if="remote" tone="info">{{ t('trash.remote') }}</Banner>
        <Banner v-if="error" tone="destructive">{{ error }}</Banner>

        <p v-if="searching" class="text-sm text-muted-foreground" role="status">
          <template v-if="entries.length === 0">
            {{ t('trash.search.none', { query: query.trim() }) }}
            <span class="block text-xs">{{ t('trash.search.scope') }}</span>
          </template>
          <template v-else>
            {{ t('trash.search.found', { sessions: sessionCount(matchedSessions), projects: t('trash.projectCount', { count: entries.length }, entries.length), query: query.trim() }) }}
            <span class="block text-xs">{{ t('trash.search.hint') }}</span>
          </template>
        </p>

        <TrashGroupCard
          v-for="entry in entries"
          :key="entry.full.project.id"
          :entry="entry"
          :query="query"
          :open="isOpen(entry.full.project.id)"
          :remote="remote"
          :busy="busy"
          :label-of="runLabel"
          @toggle="toggleGroup(entry.full.project.id)"
          @open-project="openProject(entry.full)"
          @restore-group="restoreGroup(entry)"
          @purge-group="purgeGroup(entry)"
          @restore-session="restoreSession(entry.full, $event)"
          @purge-session="purgeSession($event)"
        />
      </div>
    </div>
  </div>
</template>
