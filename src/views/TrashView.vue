<script setup lang="ts">
// Trash: sessions and projects deleted from History / Projects, grouped by
// project (lib/trash). Restore brings them back; a permanent delete (purge)
// removes logs, transcripts and links for good. Purging is local-only, so a
// remote desktop sees restore buttons only.
import { computed, onBeforeUnmount, onMounted, ref, shallowRef } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { ArchiveRestore, FolderX, RotateCw, Trash2 } from 'lucide-vue-next'
import { Badge, Banner, Button, PageHeader, SegmentedControl } from '@/components/ui'
import { invoke } from '@/lib/tauri'
import { isRemoteMode } from '@/lib/ipc/remote'
import { formatRunTimestamp, runStatusColorClass, runStatusIcon, shortModelId } from '@/lib/runDisplay'
import {
  CMD_LIST_TRASH,
  EVT_TRASH_CHANGED,
  TRASH_SORTS,
  groupTrash,
  parseTrashSort,
  sessionsLost,
  trashTotals,
  type TrashGroup,
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
    return 'deleted'
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
const sortOptions = computed(() => TRASH_SORTS.map((value) => ({ value, label: t(`trash.sort.${value}`) })))

const groups = computed(() => groupTrash(listing.value, sort.value))
const totals = computed(() => trashTotals(groups.value))

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

function restoreSession(group: TrashGroup, session: TrashSession) {
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

function restoreGroup(group: TrashGroup) {
  void act(async () => {
    if (group.projectTrashed) {
      await invoke('restore_project', { id: group.project.id })
      toast.success(t('trash.toast.projectRestored', { name: group.project.name }))
      return
    }
    for (const s of group.sessions) await invoke('restore_run', { runId: s.run.id })
    toast.success(t('trash.toast.groupRestored', { count: group.sessions.length }, group.sessions.length))
  })
}

function purgeGroup(group: TrashGroup) {
  void act(async () => {
    const count = group.purgeCount
    if (!(await confirm({
      title: t('trash.confirm.purgeTitle'),
      message: group.projectTrashed
        ? t('trash.confirm.purgeProjectMessage', { name: group.project.name, count }, count)
        : t('trash.confirm.purgeGroupMessage', { name: group.project.name, count }, count),
      confirmLabel: t('trash.purge'),
    }))) return
    const leftover = group.projectTrashed
      ? await invoke<string[]>('purge_project', { id: group.project.id })
      : await purgeRuns(group.sessions.map((s) => s.run.id))
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

function openProject(group: TrashGroup) {
  if (!group.projectTrashed) void router.push(`/projects/${group.project.id}`)
}
</script>

<template>
  <div class="flex flex-col h-full">
    <PageHeader :title="t('trash.title')" :count="totals.sessions || null" :count-title="sessionCount(totals.sessions)">
      <template #actions>
        <SegmentedControl
          :model-value="sort"
          :options="sortOptions"
          :stretch="false"
          :title="t('trash.sortBy')"
          @update:model-value="setSort"
        />
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

        <section
          v-for="group in groups"
          :key="group.project.id"
          class="rounded-lg border border-border bg-card"
        >
          <header class="flex items-center gap-2 border-b border-border/60 px-3 py-2">
            <FolderX v-if="group.projectTrashed" class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-2">
                <button
                  type="button"
                  class="truncate text-sm font-semibold"
                  :class="group.projectTrashed ? 'cursor-default' : 'hover:underline'"
                  :disabled="group.projectTrashed"
                  @click="openProject(group)"
                >
                  {{ group.project.name }}
                </button>
                <Badge v-if="group.projectTrashed" tone="warning" size="xs">{{ t('trash.projectDeleted') }}</Badge>
                <span class="text-2xs text-muted-foreground">{{ sessionCount(group.sessions.length) }}</span>
              </div>
              <p class="truncate text-2xs text-muted-foreground" :title="group.project.path">
                {{ group.project.path }}<template v-if="group.project.deleted_at"> · {{ t('trash.deletedAt', { at: formatRunTimestamp(group.project.deleted_at) }) }}</template>
              </p>
            </div>
            <Button variant="ghost" size="xs" :disabled="busy" @click="restoreGroup(group)">
              <ArchiveRestore class="h-3 w-3" :stroke-width="2" />
              {{ group.projectTrashed ? t('trash.restoreProject') : t('trash.restoreAll') }}
            </Button>
            <Button
              v-if="!remote"
              variant="destructive-ghost"
              size="xs"
              :disabled="busy"
              @click="purgeGroup(group)"
            >
              <Trash2 class="h-3 w-3" :stroke-width="2" />
              {{ group.projectTrashed ? t('trash.purgeProject') : t('trash.purgeAll') }}
            </Button>
          </header>

          <p v-if="group.sessions.length === 0" class="px-3 py-2.5 text-2xs text-muted-foreground">
            {{ t('trash.noSessions') }}
          </p>
          <div v-else class="divide-y divide-border/40">
            <div v-for="s in group.sessions" :key="s.run.id" class="flex items-center gap-3 px-3 py-2 text-xs">
              <component
                :is="runStatusIcon(s.run.status)"
                class="h-3.5 w-3.5 shrink-0"
                :class="runStatusColorClass(s.run.status)"
                :stroke-width="1.75"
              />
              <div class="min-w-0 flex-1 space-y-0.5">
                <div class="flex flex-wrap items-center gap-2">
                  <span class="truncate font-medium">{{ runLabel(s.run) }}</span>
                  <Badge size="xs">{{ s.run.engine }}</Badge>
                  <span v-if="s.run.model" class="text-2xs text-muted-foreground" :title="s.run.model">{{ shortModelId(s.run.model) }}</span>
                  <Badge v-if="s.workers.length" size="xs">{{ t('trash.workers', { count: s.workers.length }, s.workers.length) }}</Badge>
                </div>
                <p class="text-2xs text-muted-foreground">
                  {{ t('trash.deletedAt', { at: formatRunTimestamp(s.deletedAt) }) }} · {{ t('trash.createdAt', { at: formatRunTimestamp(s.run.created_at) }) }}
                </p>
              </div>
              <Button
                variant="ghost"
                size="icon-sm"
                :disabled="busy"
                :title="t('trash.restore')"
                :aria-label="t('trash.restore')"
                @click="restoreSession(group, s)"
              >
                <ArchiveRestore class="h-3.5 w-3.5" :stroke-width="1.75" />
              </Button>
              <Button
                v-if="!remote"
                variant="destructive-ghost"
                size="icon-sm"
                :disabled="busy"
                :title="t('trash.purge')"
                :aria-label="t('trash.purge')"
                @click="purgeSession(s)"
              >
                <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
              </Button>
            </div>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>
