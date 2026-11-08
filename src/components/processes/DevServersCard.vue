<script setup lang="ts">
// Dev servers a project declares in `.puppeteer/dev-servers.json`, on the Processes
// screen (`orchestration.dev_servers`): status, Start (after reading the exact command),
// Stop, and the log tail. The backend re-reads the file on every call; Start sends the
// command the user confirmed and is refused if the file says something else by then.
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { FileText, Play, RefreshCw, Server, Square } from 'lucide-vue-next'
import { AppSelect, Badge, Banner, Button, Card } from '@/components/ui'
import { invoke } from '@/lib/tauri'
import { errorText } from '@/lib/featureGate'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useProjectsStore } from '@/stores/projects'
import { useBackgroundProcessesStore } from '@/stores/backgroundProcesses'
import {
  CMD_DEVSERVER_LIST,
  CMD_DEVSERVER_LOGS,
  CMD_DEVSERVER_START,
  CMD_DEVSERVER_STOP,
  CONFIG_EXAMPLE,
  canStart,
  canStop,
  pickProjectId,
  type DevServerList,
  type DevServerLog,
  type DevServerStatus,
  type DevServerView,
} from '@/lib/devServers'

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()
const projects = useProjectsStore()
const procs = useBackgroundProcessesStore()

const projectId = ref('')
const list = ref<DevServerList | null>(null)
const error = ref<string | null>(null)
const loading = ref(false)
const busy = reactive(new Set<string>())
/** Open log panels: server name → its last read (or the error). */
const logs = reactive(new Map<string, DevServerLog | { error: string }>())

const projectOptions = computed(() => projects.projects.map((p) => ({ value: p.id, label: p.name })))

const TONE: Record<DevServerStatus, 'success' | 'warning' | 'neutral'> = {
  running: 'success',
  port_busy: 'warning',
  unknown: 'neutral',
  stopped: 'neutral',
}

async function load() {
  if (!projectId.value) return
  const id = projectId.value
  loading.value = true
  try {
    const next = await invoke<DevServerList>(CMD_DEVSERVER_LIST, { projectId: id })
    if (id !== projectId.value) return
    list.value = next
    error.value = null
  } catch (e) {
    if (id !== projectId.value) return
    list.value = null
    error.value = errorText(e, t)
  } finally {
    loading.value = false
  }
}

async function readLog(s: DevServerView) {
  try {
    logs.set(s.name, await invoke<DevServerLog>(CMD_DEVSERVER_LOGS, { projectId: projectId.value, name: s.name }))
  } catch (e) {
    logs.set(s.name, { error: errorText(e, t) })
  }
}

function logError(name: string): string | null {
  const l = logs.get(name)
  return l && 'error' in l ? l.error : null
}

function logOf(name: string): DevServerLog | null {
  const l = logs.get(name)
  return l && !('error' in l) ? l : null
}

function toggleLog(s: DevServerView) {
  if (logs.has(s.name)) logs.delete(s.name)
  else void readLog(s)
}

async function act(s: DevServerView, work: () => Promise<void>) {
  busy.add(s.name)
  try {
    await work()
  } catch (e) {
    toast.error(t('processes.devServers.failed', { error: errorText(e, t) }))
  } finally {
    busy.delete(s.name)
    await load()
    void procs.refresh()
    if (logs.has(s.name)) void readLog(s)
  }
}

async function start(s: DevServerView) {
  const ok = await confirm({
    title: t('processes.devServers.confirmStartTitle', { name: s.name }),
    message: t('processes.devServers.confirmStartMessage', { command: s.launch }),
    confirmLabel: t('processes.devServers.start'),
    variant: 'primary',
  })
  if (!ok) return
  await act(s, async () => {
    await invoke<number>(CMD_DEVSERVER_START, { projectId: projectId.value, name: s.name, command: s.launch })
    toast.success(t('processes.devServers.started', { name: s.name }))
  })
}

async function stop(s: DevServerView) {
  await act(s, async () => {
    await invoke<number>(CMD_DEVSERVER_STOP, { projectId: projectId.value, name: s.name })
  })
}

watch(
  () => projects.projects.map((p) => p.id),
  (ids) => (projectId.value = pickProjectId(ids, projectId.value)),
  { immediate: true },
)
watch(projectId, () => {
  list.value = null
  logs.clear()
  void load()
})
// A server that exits on its own (or is stopped from the list below) moves the counts.
watch(
  () => Object.values(procs.counts).join(),
  () => void load(),
)
onMounted(() => {
  if (projects.projects.length === 0) projects.fetchProjects().catch(() => {})
})
</script>

<template>
  <Card body-class="divide-y divide-border/50">
    <template #header>
      <Server class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
      <span class="text-xs font-semibold">{{ t('processes.devServers.title') }}</span>
      <span class="ml-2 min-w-0 flex-1 truncate text-2xs text-muted-foreground">{{ t('processes.devServers.hint') }}</span>
      <AppSelect
        v-if="projectOptions.length > 0"
        v-model="projectId"
        :options="projectOptions"
        size="xs"
        class="w-44 shrink-0"
      />
      <Button
        variant="ghost"
        size="icon-sm"
        :disabled="loading || !projectId"
        :title="t('processes.refresh')"
        :aria-label="t('processes.refresh')"
        @click="load()"
      >
        <RefreshCw class="h-3.5 w-3.5" :stroke-width="1.75" />
      </Button>
    </template>

    <p v-if="projectOptions.length === 0" class="px-4 py-3 text-2xs text-muted-foreground">
      {{ t('processes.devServers.noProjects') }}
    </p>

    <div v-else-if="error" class="p-3">
      <Banner tone="destructive">{{ error }}</Banner>
    </div>

    <div v-else-if="list?.missing" class="space-y-2 px-4 py-3 text-2xs text-muted-foreground">
      <p>{{ t('processes.devServers.missing', { path: list.config_path }) }}</p>
      <pre class="overflow-x-auto rounded bg-muted/50 p-2 font-mono">{{ CONFIG_EXAMPLE }}</pre>
      <p>{{ t('processes.devServers.rules') }}</p>
    </div>

    <template v-else-if="list">
      <p v-if="list.servers.length === 0" class="px-4 py-3 text-2xs text-muted-foreground">
        {{ t('processes.devServers.empty', { path: list.config_path }) }}
      </p>

      <div v-for="s in list.servers" :key="s.name" class="px-3 py-2">
        <div class="flex items-center gap-2">
          <Badge :tone="TONE[s.status]" size="xs" class="shrink-0">{{ t(`processes.devServers.status.${s.status}`) }}</Badge>
          <div class="min-w-0 flex-1">
            <span class="block truncate text-sm leading-tight">
              {{ s.name }}
              <span v-if="s.port" class="text-2xs text-muted-foreground">:{{ s.port }}</span>
            </span>
            <span v-if="s.description" class="block truncate text-2xs text-muted-foreground leading-tight">{{ s.description }}</span>
          </div>
          <span v-if="s.pids?.length" class="shrink-0 text-2xs text-muted-foreground tabular-nums">
            {{ t('processes.devServers.pids', { pids: s.pids.join(', ') }) }}
          </span>
          <Button
            variant="ghost"
            size="icon-sm"
            :title="t('processes.devServers.log')"
            :aria-label="t('processes.devServers.log')"
            :aria-expanded="logs.has(s.name)"
            @click="toggleLog(s)"
          >
            <FileText class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
          <Button v-if="canStart(s)" variant="outline" size="xs" :disabled="busy.has(s.name)" @click="start(s)">
            <Play class="h-3 w-3" :stroke-width="2" />
            {{ t('processes.devServers.start') }}
          </Button>
          <Button v-else-if="canStop(s)" variant="outline" size="xs" :disabled="busy.has(s.name)" @click="stop(s)">
            <Square class="h-3 w-3" :stroke-width="2" />
            {{ t('processes.actions.stop') }}
          </Button>
        </div>
        <p class="mt-1 truncate pl-1 font-mono text-2xs text-muted-foreground" :title="s.launch">{{ s.launch }}</p>
        <p v-if="s.status === 'port_busy'" class="mt-1 pl-1 text-2xs text-warning">
          {{ t('processes.devServers.portBusy', { port: s.port }) }}
        </p>
        <p v-if="logError(s.name)" class="mt-1 pl-1 text-2xs text-muted-foreground">{{ logError(s.name) }}</p>
        <template v-else-if="logOf(s.name)">
          <p v-if="logOf(s.name)?.truncated" class="mt-1 pl-1 text-2xs text-muted-foreground">
            {{ t('processes.devServers.logTruncated') }}
          </p>
          <pre class="mt-1 max-h-64 overflow-auto rounded bg-muted/50 p-2 font-mono text-2xs whitespace-pre-wrap">{{
            logOf(s.name)?.text || t('processes.devServers.logEmpty')
          }}</pre>
        </template>
      </div>

      <div v-if="list.problems.length > 0" class="px-4 py-2 text-2xs text-warning">
        <p>{{ t('processes.devServers.problems', { path: list.config_path }) }}</p>
        <ul class="list-disc pl-4">
          <li v-for="(p, i) in list.problems" :key="i">{{ p }}</li>
        </ul>
      </div>
    </template>
  </Card>
</template>
