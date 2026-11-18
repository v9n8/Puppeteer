<script setup lang="ts">
// "Session" tab of the Running screen (P7): every Session running or waiting for
// the user, in every Project, with multi-select stop. Rows come from
// lib/runningSessions; stopping goes through `cancel_run`, one call per Session
// and per worker so a failure is reported for that one alone.
import { computed, onUnmounted, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import {
  Activity, AlertTriangle, ArrowUpRight, CircleCheck, CircleSlash, CircleStop, CircleX, Ellipsis,
  LoaderCircle, Network, ShieldAlert,
} from 'lucide-vue-next'
import { AppSelect, Button, DropdownItem, DropdownMenu, EmptyState, SegmentedControl, Skeleton } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useRunningRows } from '@/composables/useRunningRows'
import { useRunsStore } from '@/stores/runs'
import { useProjectsStore } from '@/stores/projects'
import { formatAge } from '@/lib/backgroundProcesses'
import { engineLabel } from '@/lib/skillRows'
import {
  filterRunning, finishedRunning, runningCounts, runningProjects, sortRunning, stopTargets,
  type RunningFilter, type RunningRow, type RunningStatus,
} from '@/lib/runningSessions'

const emit = defineEmits<{ viewProcesses: [runId: string] }>()

const { t } = useI18n()
const router = useRouter()
const { confirm } = useConfirm()
const { toast } = useToast()
const runsStore = useRunsStore()
const projectsStore = useProjectsStore()
const { rows, openRow, openRowInWindow } = useRunningRows()

const status = ref<RunningFilter['status']>('all')
const projectId = ref('')
const filter = computed<RunningFilter>(() => ({ status: status.value, projectId: projectId.value || null }))

const counts = computed(() => runningCounts(rows.value, filter.value.projectId))
const shown = computed(() => sortRunning(filterRunning(rows.value, filter.value)))
const finished = computed(() => finishedRunning(rows.value, filter.value))

const statusOptions = computed(() => [
  { value: 'all' as const, label: `${t('processes.running.filter.all')} ${counts.value.all}` },
  { value: 'wait' as const, label: `${t('processes.running.filter.wait')} ${counts.value.wait}` },
  { value: 'run' as const, label: `${t('processes.running.filter.run')} ${counts.value.run}` },
])
const projectOptions = computed(() => [
  { value: '', label: t('processes.running.filter.allProjects') },
  ...runningProjects(rows.value).map((p) => ({ value: p.id, label: p.name })),
])
// A Project that no longer has a Session would leave the filter on an empty list.
watch(projectOptions, (opts) => {
  if (projectId.value && !opts.some((o) => o.value === projectId.value)) projectId.value = ''
})

// Ages tick on their own: the rows carry a start time, not an elapsed time.
const now = ref(Date.now())
const tick = setInterval(() => (now.value = Date.now()), 15_000)
onUnmounted(() => clearInterval(tick))
const age = (r: RunningRow) => formatAge(r.startedAt, now.value)

// ── Selection ──────────────────────────────────────────────────────────────
const selected = reactive(new Set<string>())
watch(rows, (list) => {
  const ids = new Set(list.map((r) => r.runId))
  selected.forEach((id) => !ids.has(id) && selected.delete(id))
})
const shownIds = computed(() => shown.value.map((r) => r.runId))
const allSelected = computed(() => shownIds.value.length > 0 && shownIds.value.every((id) => selected.has(id)))
const picked = computed(() => stopTargets(selected, shown.value))

function toggle(id: string) {
  if (selected.has(id)) selected.delete(id)
  else selected.add(id)
}
function toggleAll() {
  if (allSelected.value) shownIds.value.forEach((id) => selected.delete(id))
  else shownIds.value.forEach((id) => selected.add(id))
}
// Space on a focused row; ⌘A only while focus is in the table, so it never
// steals Select All from a text field elsewhere on the page.
function onRowKey(e: KeyboardEvent, id: string) {
  if (e.key === ' ' && e.target === e.currentTarget) {
    e.preventDefault()
    toggle(id)
  }
}
function onTableKey(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'a') {
    e.preventDefault()
    shownIds.value.forEach((id) => selected.add(id))
  }
}

// ── Stopping ───────────────────────────────────────────────────────────────
const stopping = reactive(new Set<string>())

/** Cancel each Session; the ids that failed come back, with a toast per failure. */
async function cancelAll(targets: RunningRow[]): Promise<Set<string>> {
  targets.forEach((r) => stopping.add(r.runId))
  const results = await Promise.allSettled(targets.map((r) => runsStore.cancelRun(r.runId)))
  const failed = new Set<string>()
  results.forEach((res, i) => {
    stopping.delete(targets[i].runId)
    if (res.status === 'fulfilled') return
    failed.add(targets[i].runId)
    toast.error(t('processes.running.toast.failed', { label: targets[i].title, error: String(res.reason) }))
  })
  const ok = targets.length - failed.size
  if (ok > 0) toast.success(t('processes.running.toast.stopped', { n: ok }, ok))
  return failed
}

async function stopOne(r: RunningRow) {
  await cancelAll([r])
}

function nameList(list: RunningRow[]): string {
  const max = 6
  const lines = list.slice(0, max).map((r) => `• ${r.title}`)
  if (list.length > max) lines.push(t('processes.running.confirm.more', { n: list.length - max }))
  return lines.join('\n')
}

async function stopSelected() {
  const { sessions, workers } = picked.value
  if (sessions.length === 0) return
  const names = nameList(sessions)
  const ok = await confirm({
    title: t('processes.running.confirm.title', { n: sessions.length }, sessions.length),
    message: workers.length > 0
      ? t('processes.running.confirm.withWorkers', { w: workers.length, names })
      : t('processes.running.confirm.message', { names }),
    confirmLabel: t('processes.running.selection.stop', { n: sessions.length }, sessions.length),
  })
  if (!ok) return
  const failed = await cancelAll([...sessions, ...workers])
  // What stopped leaves the selection; what failed stays picked to retry.
  sessions.forEach((r) => !failed.has(r.runId) && selected.delete(r.runId))
}

async function stopAll() {
  const { sessions, workers } = stopTargets(shownIds.value, shown.value)
  if (sessions.length === 0) return
  const ok = await confirm({
    title: t('processes.running.confirm.allTitle'),
    message: t('processes.running.confirm.allMessage', { n: sessions.length, names: nameList(sessions) }),
    confirmLabel: t('processes.running.action.stopAll'),
  })
  if (!ok) return
  await cancelAll([...sessions, ...workers])
}

// ── Row presentation ───────────────────────────────────────────────────────
const STATUS_ICON = { wait: ShieldAlert, run: LoaderCircle, done: CircleCheck, err: CircleX, stop: CircleSlash }
const STATUS_TONE: Record<RunningStatus, string> = {
  wait: 'text-warning', run: 'text-primary', done: 'text-success', err: 'text-destructive', stop: 'text-muted-foreground',
}

function subtitle(r: RunningRow): string {
  const m = runsStore.runMeta.get(r.runId)
  const kind = r.isConductor ? 'Conductor' : m?.conductor_run_id ? 'Worker' : engineLabel(r.engine || 'claude')
  const engine = r.isConductor || m?.conductor_run_id ? engineLabel(r.engine || 'claude') : ''
  return [r.projectName, kind, engine, r.model].filter(Boolean).join(' · ')
}

function waitingText(r: RunningRow): string {
  if (r.waitingOn === 'user') return t('processes.running.waiting.user')
  if (r.waitingOn === 'agent') return t('processes.running.waiting.agent')
  if (r.waitingOn === 'worker') return t('processes.running.waiting.worker', { n: r.workers.length })
  return '—'
}

function statusLabel(r: RunningRow): string {
  return t(`processes.running.status.${r.status}`)
}

function openProjects() {
  router.push('/projects').catch(() => {})
}

const loading = computed(() => projectsStore.loading && projectsStore.projects.length === 0)
const empty = computed(() => rows.value.filter((r) => r.status === 'run' || r.status === 'wait').length === 0 && finished.value.length === 0)

// The page header owns the "Stop all" button and reads these through a template ref.
const canStopAll = computed(() => !loading.value && !empty.value && shown.value.length > 0)
defineExpose({ canStopAll, stopAll })
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- Loading -->
    <div v-if="loading" class="space-y-2 p-6">
      <Skeleton v-for="i in 5" :key="i" class="h-[52px] w-full rounded-lg" />
    </div>

    <!-- Nothing running -->
    <EmptyState
      v-else-if="empty"
      fill
      :icon="Activity"
      :title="t('processes.running.empty.title')"
      :hint="t('processes.running.empty.hint')"
    >
      <Button variant="outline" @click="openProjects">{{ t('processes.running.empty.openProjects') }}</Button>
    </EmptyState>

    <template v-else>
      <!-- Filters -->
      <div class="flex shrink-0 flex-wrap items-center gap-2 px-6 pt-4">
        <SegmentedControl v-model="status" :options="statusOptions" :stretch="false" />
        <AppSelect
          v-model="projectId"
          :options="projectOptions"
          :aria-label="t('processes.running.filter.projectTitle')"
          :title="t('processes.running.filter.projectTitle')"
          class="w-44"
        />
        <span class="ml-auto text-2xs text-muted-foreground max-[901px]:hidden">{{ t('processes.running.filter.sort') }}</span>
      </div>

      <!-- Selection bar -->
      <div
        v-if="picked.sessions.length > 0"
        class="mx-6 mt-3 flex shrink-0 items-center gap-3 rounded-lg border border-border bg-accent/60 px-3 py-2 text-sm"
      >
        <span class="font-medium">{{ t('processes.running.selection.count', { n: picked.sessions.length }, picked.sessions.length) }}</span>
        <span v-if="picked.workers.length > 0" class="text-muted-foreground">
          {{ t('processes.running.selection.workers', { n: picked.workers.length }, picked.workers.length) }}
        </span>
        <span class="flex-1" />
        <Button variant="ghost" @click="selected.clear()">{{ t('processes.running.selection.clear') }}</Button>
        <Button variant="destructive" :disabled="picked.sessions.length === 0" @click="stopSelected">
          <CircleStop class="h-3.5 w-3.5" :stroke-width="1.75" />
          {{ t('processes.running.selection.stop', { n: picked.sessions.length }, picked.sessions.length) }}
        </Button>
      </div>

      <div class="flex-1 overflow-auto p-6" @keydown="onTableKey">
        <div class="rounded-[10px] border border-border bg-card">
          <!-- Header -->
          <div class="flex min-w-0 items-center gap-2 rounded-t-[10px] border-b border-border bg-muted px-3 text-[13px] font-semibold" style="height: 36px">
            <label class="-mx-1.5 flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center">
              <input
                type="checkbox"
                class="h-4 w-4 cursor-pointer accent-primary"
                :checked="allSelected"
                :disabled="shown.length === 0"
                :aria-label="t('processes.running.action.selectAll')"
                @change="toggleAll"
              />
            </label>
            <span class="w-[108px] shrink-0 max-[901px]:w-5 max-[901px]:overflow-hidden max-[901px]:text-transparent">{{ t('processes.running.col.status') }}</span>
            <span class="min-w-0 flex-1">{{ t('processes.running.col.session') }}</span>
            <span class="w-[230px] shrink min-w-0 max-[901px]:w-[150px]">{{ t('processes.running.col.doing') }}</span>
            <span class="w-[72px] shrink-0 text-right">{{ t('processes.running.col.age') }}</span>
            <span class="w-[84px] shrink-0 max-[901px]:hidden">{{ t('processes.running.col.waiting') }}</span>
            <span class="w-[64px] shrink-0 text-right max-[901px]:hidden">{{ t('processes.running.col.workers') }}</span>
            <span class="w-[132px] shrink-0 max-[901px]:w-[96px]" />
          </div>

          <p v-if="shown.length === 0" class="px-3 py-6 text-center text-sm text-muted-foreground">
            {{ t('processes.running.empty.title') }}
          </p>

          <template v-for="r in shown" :key="r.runId">
            <div
              tabindex="0"
              class="flex min-h-[52px] min-w-0 items-center gap-2 border-t border-border px-3 py-1.5 first:border-t-0 hover:bg-accent/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
              :class="selected.has(r.runId) && 'bg-accent'"
              @keydown="onRowKey($event, r.runId)"
            >
              <label class="-mx-1.5 flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center">
                <input
                  type="checkbox"
                  class="h-4 w-4 cursor-pointer accent-primary"
                  :checked="selected.has(r.runId)"
                  :aria-label="t('processes.running.action.selectRow', { title: r.title })"
                  @change="toggle(r.runId)"
                />
              </label>
              <span class="flex w-[108px] shrink-0 items-center gap-1.5 text-[13px] font-medium max-[901px]:w-5" :class="STATUS_TONE[r.status]" :title="statusLabel(r)">
                <component :is="STATUS_ICON[r.status]" class="h-4 w-4 shrink-0" :class="r.status === 'run' && 'animate-spin'" :stroke-width="2" />
                <span class="truncate max-[901px]:hidden">{{ statusLabel(r) }}</span>
              </span>
              <span class="min-w-0 flex-1">
                <span class="flex items-center gap-1.5">
                  <span class="truncate text-sm" :title="r.title">{{ r.title }}</span>
                  <AlertTriangle v-if="r.stuck" class="h-3.5 w-3.5 shrink-0 text-warning" :stroke-width="2" :title="t('processes.running.stuck')" />
                </span>
                <span class="block truncate text-2xs text-muted-foreground" :title="subtitle(r)">{{ subtitle(r) }}</span>
              </span>
              <span class="w-[230px] min-w-0 shrink text-[13px] max-[901px]:w-[150px]">
                <template v-if="r.doing.kind === 'permission'">
                  <span class="block truncate" :title="r.doing.arg">
                    <span class="font-medium text-warning">{{ t('processes.running.doing.permission', { tool: r.doing.name }) }}</span>
                    {{ ' ' }}<span class="font-mono text-muted-foreground">{{ r.doing.arg }}</span>
                  </span>
                </template>
                <template v-else-if="r.doing.kind === 'tool'">
                  <span class="block truncate" :title="r.doing.name + ' ' + r.doing.arg">
                    <span class="font-medium">{{ r.doing.name }}</span>
                    {{ ' ' }}<span class="font-mono text-muted-foreground">{{ r.doing.arg }}</span>
                  </span>
                </template>
                <span v-else-if="r.doing.kind === 'workers'" class="block truncate text-muted-foreground">
                  {{ t('processes.running.doing.workers', { n: r.workers.length }, r.workers.length) }}
                </span>
                <span v-else class="text-muted-foreground">—</span>
              </span>
              <span class="w-[72px] shrink-0 text-right text-[13px] tabular-nums text-muted-foreground">{{ age(r) }}</span>
              <span class="w-[84px] shrink-0 whitespace-nowrap text-[13px] max-[901px]:hidden" :class="r.waitingOn === 'user' ? 'font-medium text-warning' : 'text-muted-foreground'">{{ waitingText(r) }}</span>
              <span class="flex w-[64px] shrink-0 items-center justify-end gap-1 text-[13px] text-muted-foreground max-[901px]:hidden">
                <template v-if="r.workers.length > 0"><Network class="h-3.5 w-3.5" :stroke-width="2" />{{ r.workers.length }}</template>
              </span>
              <span class="flex w-[132px] shrink-0 items-center justify-end gap-1 max-[901px]:w-[96px]">
                <Button variant="outline" size="xs" :title="t('processes.running.action.openTitle')" @click="openRow(r)">
                  {{ t('processes.running.action.open') }}
                </Button>
                <Button
                  variant="ghost" size="icon-sm"
                  :disabled="stopping.has(r.runId)"
                  :title="t('processes.running.action.stopTitle')"
                  :aria-label="t('processes.running.action.stop')"
                  @click="stopOne(r)"
                >
                  <CircleStop class="h-4 w-4" :stroke-width="1.75" />
                </Button>
                <DropdownMenu>
                  <template #trigger>
                    <Button variant="ghost" size="icon-sm" :title="t('processes.running.action.more')" :aria-label="t('processes.running.action.more')">
                      <Ellipsis class="h-4 w-4" :stroke-width="1.75" />
                    </Button>
                  </template>
                  <DropdownItem @click="openRow(r)">{{ t('processes.running.menu.open') }}</DropdownItem>
                  <DropdownItem @click="openRowInWindow(r)">{{ t('processes.running.menu.openWindow') }}</DropdownItem>
                  <DropdownItem @click="emit('viewProcesses', r.runId)">{{ t('processes.running.menu.viewProcesses') }}</DropdownItem>
                  <DropdownItem variant="destructive" :disabled="stopping.has(r.runId)" @click="stopOne(r)">
                    {{ t('processes.running.menu.stop') }}
                  </DropdownItem>
                </DropdownMenu>
              </span>
            </div>

            <!-- Workers of a conductor: indented, no checkbox, own actions. -->
            <div
              v-for="w in r.workers"
              :key="w.runId"
              class="flex min-h-[44px] min-w-0 items-center gap-2 border-t border-border py-1.5 pl-12 pr-3 hover:bg-accent/50"
            >
              <span class="flex w-[108px] shrink-0 items-center gap-1.5 text-[13px] font-medium max-[901px]:w-5" :class="STATUS_TONE[w.status]" :title="statusLabel(w)">
                <component :is="STATUS_ICON[w.status]" class="h-4 w-4 shrink-0" :class="w.status === 'run' && 'animate-spin'" :stroke-width="2" />
                <span class="truncate max-[901px]:hidden">{{ statusLabel(w) }}</span>
              </span>
              <span class="min-w-0 flex-1">
                <span class="block truncate text-sm" :title="w.title">{{ w.title }}</span>
                <span class="block truncate text-2xs text-muted-foreground">{{ subtitle(w) }}</span>
              </span>
              <span class="w-[230px] min-w-0 shrink truncate text-[13px] max-[901px]:w-[150px]">
                <template v-if="w.doing.kind === 'permission' || w.doing.kind === 'tool'">
                  <span class="font-medium" :class="w.doing.kind === 'permission' && 'text-warning'">
                    {{ w.doing.kind === 'permission' ? t('processes.running.doing.permission', { tool: w.doing.name }) : w.doing.name }}
                  </span>
                  {{ ' ' }}<span class="font-mono text-muted-foreground">{{ w.doing.arg }}</span>
                </template>
                <span v-else class="text-muted-foreground">—</span>
              </span>
              <span class="w-[72px] shrink-0 text-right text-[13px] tabular-nums text-muted-foreground">{{ age(w) }}</span>
              <span class="w-[84px] shrink-0 max-[901px]:hidden" />
              <span class="w-[64px] shrink-0 max-[901px]:hidden" />
              <span class="flex w-[132px] shrink-0 items-center justify-end gap-1 max-[901px]:w-[96px]">
                <Button variant="ghost" size="icon-sm" :title="t('processes.running.action.openWorker')" :aria-label="t('processes.running.action.openWorker')" @click="openRow(w)">
                  <ArrowUpRight class="h-4 w-4" :stroke-width="1.75" />
                </Button>
                <Button
                  variant="ghost" size="icon-sm"
                  :disabled="stopping.has(w.runId)"
                  :title="t('processes.running.action.stopWorker')"
                  :aria-label="t('processes.running.action.stopWorker')"
                  @click="stopOne(w)"
                >
                  <CircleStop class="h-4 w-4" :stroke-width="1.75" />
                </Button>
              </span>
            </div>
          </template>

          <!-- Finished, not opened yet (the dock's "done" notification rule). -->
          <template v-if="finished.length > 0">
            <div class="flex min-h-[36px] items-center border-t border-border px-3 text-[13px] font-semibold">
              {{ t('processes.running.finished', { n: finished.length }) }}
            </div>
            <div
              v-for="r in finished"
              :key="r.runId"
              class="flex min-h-[52px] min-w-0 items-center gap-2 border-t border-border px-3 py-1.5 text-muted-foreground hover:bg-accent/50"
            >
              <span class="h-4 w-4 shrink-0" />
              <span class="flex w-[108px] shrink-0 items-center gap-1.5 text-[13px] font-medium max-[901px]:w-5" :class="STATUS_TONE[r.status]" :title="statusLabel(r)">
                <component :is="STATUS_ICON[r.status]" class="h-4 w-4 shrink-0" :stroke-width="2" />
                <span class="truncate max-[901px]:hidden">{{ statusLabel(r) }}</span>
              </span>
              <span class="min-w-0 flex-1">
                <span class="block truncate text-sm text-foreground" :title="r.title">{{ r.title }}</span>
                <span class="block truncate text-2xs" :title="subtitle(r)">{{ subtitle(r) }}</span>
              </span>
              <span class="w-[230px] shrink max-[901px]:w-[150px]" />
              <span class="w-[72px] shrink-0 text-right text-[13px] tabular-nums">{{ age(r) }}</span>
              <span class="w-[84px] shrink-0 max-[901px]:hidden" />
              <span class="w-[64px] shrink-0 max-[901px]:hidden" />
              <span class="flex w-[132px] shrink-0 justify-end max-[901px]:w-[96px]">
                <Button variant="outline" size="xs" :title="t('processes.running.action.openTitle')" @click="openRow(r)">
                  {{ t('processes.running.action.open') }}
                </Button>
              </span>
            </div>
          </template>
        </div>
      </div>
    </template>
  </div>
</template>
