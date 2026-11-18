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
import { Banner, Button, EmptyState, PageHeader } from '@/components/ui'
import { useFeaturesStore } from '@/stores/features'
import TrashGroupCard from '@/components/trash/TrashGroupCard.vue'
import TrashToolbar from '@/components/trash/TrashToolbar.vue'
import { invoke } from '@/lib/tauri'
import { isRemoteMode } from '@/lib/ipc/remote'
import { errorText } from '@/lib/featureGate'
import {
  CMD_SEARCH_SESSIONS,
  groupSessionHits,
  SESSION_SEARCH_FEATURE,
  type SessionHitGroup,
  type SessionSearch,
} from '@/lib/sessionSearch'
import {
  CMD_LIST_TRASH,
  contentFilter,
  EVT_TRASH_CHANGED,
  filterTrash,
  groupActionTargets,
  groupTrash,
  initialOpenGroups,
  parseTrashSearchMode,
  parseTrashSort,
  sessionsLost,
  trashTotals,
  type TrashContentHit,
  type TrashEntry,
  type TrashListing,
  type TrashRun,
  type TrashSearchMode,
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
const features = useFeaturesStore()

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

// Name search filters as you type; Content search runs on Enter against the
// session index (search_sessions, scope trash). The mode is a per-viewer
// convenience; it falls back to Name while the index is switched off.
const MODE_KEY = 'puppeteer.trashSearchMode'
function loadMode(): TrashSearchMode {
  try {
    return parseTrashSearchMode(localStorage.getItem(MODE_KEY))
  } catch {
    return 'name'
  }
}
const storedMode = ref<TrashSearchMode>(loadMode())
const contentLocked = computed(() => features.loaded && !features.isOn(SESSION_SEARCH_FEATURE))
const mode = computed<TrashSearchMode>(() => (storedMode.value === 'content' && !contentLocked.value ? 'content' : 'name'))
function setMode(value: TrashSearchMode) {
  storedMode.value = value
  try {
    localStorage.setItem(MODE_KEY, value)
  } catch {
    /* storage unavailable: the choice lasts for this view only */
  }
  resetContent()
}

type ContentState = 'idle' | 'searching' | 'done' | 'error'
const contentState = ref<ContentState>('idle')
const contentGroups = shallowRef<SessionHitGroup[]>([])
const contentTruncated = ref(false)
const contentError = ref('')
/** The text of the last content search: what the results and their highlights are for. */
const searchedQuery = ref('')
let contentSeq = 0
function resetContent() {
  contentSeq++
  contentState.value = 'idle'
  contentGroups.value = []
  contentTruncated.value = false
  contentError.value = ''
}
watch(query, resetContent)

async function runContentSearch() {
  const q = query.value.trim()
  if (!q || contentState.value === 'searching') return
  const mine = ++contentSeq
  contentState.value = 'searching'
  searchedQuery.value = q
  try {
    const r = await invoke<SessionSearch>(CMD_SEARCH_SESSIONS, { query: q, projectId: null, scope: 'trash' })
    if (mine !== contentSeq) return
    contentGroups.value = groupSessionHits(r.hits)
    contentTruncated.value = r.truncated
    contentState.value = 'done'
  } catch (e) {
    if (mine !== contentSeq) return
    contentError.value = errorText(e, t)
    contentState.value = 'error'
  }
}

const contentDone = computed(() => mode.value === 'content' && contentState.value === 'done')
const nameSearching = computed(() => mode.value === 'name' && query.value.trim() !== '')
/** A search narrows the list: every block with a result is open. */
const searching = computed(() => nameSearching.value || contentDone.value)
const contentResult = computed(() => (contentDone.value ? contentFilter(groups.value, contentGroups.value) : null))
const entries = computed(() => contentResult.value?.entries ?? filterTrash(groups.value, nameSearching.value ? query.value : '', runLabel))
const matchedSessions = computed(() => entries.value.reduce((n, e) => n + e.group.sessions.length, 0))
const contentHits = computed<ReadonlyMap<string, TrashContentHit> | undefined>(() => contentResult.value?.hits)
/** Name-search highlights belong to the Name mode only. */
const nameQuery = computed(() => (mode.value === 'name' ? query.value : ''))

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
watch([query, contentGroups], () => (foldedWhileSearching.value = new Set()))
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
          query: (contentDone.value ? searchedQuery.value : query.value).trim(),
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
        <Button v-if="!remote" variant="destructive-solid" size="xs" :disabled="busy || groups.length === 0" @click="emptyTrash">
          <Trash2 class="h-3.5 w-3.5" :stroke-width="2" />
          {{ t('trash.emptyTrash') }}
        </Button>
      </template>
    </PageHeader>
    <TrashToolbar
      v-model:query="query"
      :sort="sort"
      :mode="mode"
      :content-locked="contentLocked"
      :searching="contentState === 'searching'"
      @update:sort="setSort"
      @update:mode="setMode"
      @submit="runContentSearch"
    />

    <div class="flex-1 overflow-auto p-6">
      <EmptyState
        v-if="!loading && !error && groups.length === 0"
        fill
        :icon="Trash2"
        :title="t('trash.empty.title')"
        :hint="t('trash.empty.hint')"
      />

      <div v-else class="max-w-4xl space-y-4">
        <p class="text-xs text-muted-foreground">{{ t('trash.hint') }}</p>
        <Banner v-if="remote" tone="info">{{ t('trash.remote') }}</Banner>
        <Banner v-if="error" tone="destructive">{{ error }}</Banner>
        <Banner v-if="mode === 'content' && contentState === 'error'" tone="destructive" live>
          {{ t('trash.content.error', { error: contentError }) }}
          <template #actions>
            <Button variant="ghost" size="xs" @click="runContentSearch">{{ t('trash.content.retry') }}</Button>
          </template>
        </Banner>

        <p v-if="nameSearching" class="text-sm text-muted-foreground" role="status">
          <template v-if="entries.length === 0">
            {{ t('trash.search.none', { query: query.trim() }) }}
            <span class="block text-xs">{{ t('trash.search.scope') }}</span>
          </template>
          <template v-else>
            {{ t('trash.search.found', { sessions: sessionCount(matchedSessions), projects: t('trash.projectCount', { count: entries.length }, entries.length), query: query.trim() }) }}
            <span class="block text-xs">{{ t('trash.search.hint') }}</span>
          </template>
        </p>
        <p v-else-if="mode === 'content' && contentState === 'idle' && query.trim()" class="text-sm text-muted-foreground" role="status">
          {{ t('trash.content.typing') }}
        </p>
        <p v-else-if="mode === 'content' && contentState === 'searching'" class="text-sm text-muted-foreground" role="status">
          {{ t('trash.content.searching', { query: searchedQuery, count: listing.runs.length }, listing.runs.length) }}
        </p>
        <p v-else-if="contentDone" class="text-sm text-muted-foreground" role="status">
          <template v-if="entries.length === 0">
            {{ t('trash.content.none', { query: searchedQuery }) }}
            <span class="block text-xs">{{ t('trash.content.noneHint') }}</span>
          </template>
          <template v-else>
            {{ t('trash.content.found', { sessions: sessionCount(matchedSessions), projects: t('trash.projectCount', { count: entries.length }, entries.length), query: searchedQuery }) }}
            <span class="block text-xs">{{ t('trash.content.hint') }}</span>
            <span v-if="contentTruncated" class="block text-xs">{{ t('trash.content.truncated', { count: 200 }) }}</span>
          </template>
        </p>

        <TrashGroupCard
          v-for="entry in entries"
          :key="entry.full.project.id"
          :entry="entry"
          :query="nameQuery"
          :content-hits="contentHits"
          :content-query="searchedQuery"
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
