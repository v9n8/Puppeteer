<script setup lang="ts">
// Right column of the Projects page (UX-49): the selected project's header,
// its tabs and the Overview tab. Management only — nothing here opens a
// session (D3); the project settings stay on their own page until UX-50.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import {
  Cloud, CloudCog, Code2, Eye, EyeOff, FolderOpen, GanttChartSquare, Github, Gitlab, HardDrive,
  Settings, SquareTerminal, Trash2,
} from 'lucide-vue-next'
import { Badge, Button } from '@/components/ui'
import DetailHeader from '@/components/listDetail/DetailHeader.vue'
import DetailSection from '@/components/listDetail/DetailSection.vue'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { invoke } from '@/lib/tauri'
import { openGanttWindow } from '@/lib/ganttWindow'
import { composerDefaults } from '@/lib/composerChips'
import { projectIcon, projectTileClass } from '@/lib/projectAppearance'
import { PROJECT_TABS, type ProjectTab } from '@/lib/projectList'
import { formatRunTimestamp, relativeAge } from '@/lib/runDisplay'
import { useProjectsStore, type Project, type Repo } from '@/stores/projects'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useAwsAccountsStore } from '@/stores/awsAccounts'
import { useAzureAccountsStore } from '@/stores/azureAccounts'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { useFeaturesStore } from '@/stores/features'
import { useGithubAccountsStore } from '@/stores/githubAccounts'
import { useGitlabAccountsStore } from '@/stores/gitlabAccounts'
import { useLiveRunsStore } from '@/stores/liveRuns'
import { useServersStore, type ProjectServer } from '@/stores/servers'
import { useWorkspaceTabsStore } from '@/stores/workspaceTabs'

const props = defineProps<{ project: Project }>()
const tab = defineModel<ProjectTab>('tab', { required: true })

defineSlots<{
  /** Before the project tile (e.g. the button that opens the list on a narrow window). */
  leading?: () => unknown
}>()

const { t } = useI18n()
const router = useRouter()
const { confirm } = useConfirm()
const { toast } = useToast()
const store = useProjectsStore()
const appSettings = useAppSettingsStore()
const claudeStore = useClaudeAccountsStore()
const ghStore = useGithubAccountsStore()
const glStore = useGitlabAccountsStore()
const awsStore = useAwsAccountsStore()
const azureStore = useAzureAccountsStore()
const serversStore = useServersStore()
const features = useFeaturesStore()
const live = useLiveRunsStore()
const tabsStore = useWorkspaceTabsStore()

appSettings.ensureLoaded().catch(() => {})
if (!claudeStore.accounts.length) claudeStore.fetch().catch(() => {})

const settingsPath = (section: string) => `/projects/${props.project.id}/settings?section=${section}`

// --- Repos and mapped servers (no bulk endpoint: loaded per selected project) ---
const repos = ref<Repo[] | null>(null)
const servers = ref<ProjectServer[]>([])
watch(() => props.project.id, async (id) => {
  repos.value = null
  servers.value = []
  const [r, s] = await Promise.all([
    store.listRepos(id).catch(() => [] as Repo[]),
    serversStore.listForProject(id).catch(() => [] as ProjectServer[]),
  ])
  // A slower answer for a project the user already left is dropped.
  if (props.project.id !== id) return
  repos.value = r
  servers.value = s
}, { immediate: true })

function repoCoords(repo: Repo): string {
  if (repo.provider === 'gitlab') return repo.gitlab_project_path || repo.path
  return repo.github_owner && repo.github_repo ? `${repo.github_owner}/${repo.github_repo}` : repo.path
}

// --- Numbers ---
const runningCount = computed(() => {
  let n = 0
  live.sessions.forEach((s) => {
    if (s.projectId === props.project.id && s.status === 'running') n += 1
  })
  return n
})
const lastSession = computed(() => props.project.last_used_at)

// --- Session defaults: what a new session of this project starts with ---
const defaults = computed(() => composerDefaults({ settings: appSettings.settings, project: props.project.session_defaults }))
const engineLabel = computed(() => (defaults.value.engine === 'codex' ? 'Codex' : 'Claude'))
const modelLabel = computed(() => {
  const model = defaults.value.engine === 'codex' ? defaults.value.models.codex : defaults.value.models.claude
  return model || t('settings.ai.modelDefault')
})
const claudeAccountLabel = computed(() => {
  const linked = claudeStore.accounts.find((a) => a.id === props.project.claude_account_id)
  if (linked) return linked.label
  const def = claudeStore.accounts.find((a) => a.is_default)
  return def ? t('projectDetail.claudeDefaultOption', { label: def.label }) : t('projectDetail.sessionDefaults.noAccount')
})

// --- Connections: the default account of each provider (+N other enabled) and mapped servers ---
type Tone = 'info' | 'warning' | 'primary' | 'success' | 'error' | 'neutral'
interface Chip { key: string; icon: typeof Github; tone: Tone; label: string; extra: number; title: string }
const connections = computed<Chip[]>(() => {
  const p = props.project
  const unknown = t('projects.unknownAccount')
  const out: Chip[] = []
  if (p.github_account_id) {
    const label = ghStore.accounts.find((a) => a.id === p.github_account_id)?.label ?? unknown
    out.push({ key: 'gh', icon: Github, tone: 'info', label, extra: p.github_account_ids.length - 1, title: `GitHub: ${label}` })
  }
  if (p.gitlab_account_id) {
    const label = glStore.accounts.find((a) => a.id === p.gitlab_account_id)?.label ?? unknown
    out.push({ key: 'gl', icon: Gitlab, tone: 'warning', label, extra: p.gitlab_account_ids.length - 1, title: `GitLab: ${label}` })
  }
  const aws = awsStore.accounts.find((a) => a.id === p.aws_account_id)
  if (aws) {
    out.push({
      key: 'aws', icon: Cloud, tone: 'primary', label: aws.label, extra: p.aws_account_ids.length - 1,
      title: t('projects.awsChipTitle', { label: aws.label, region: aws.region }),
    })
  }
  const azure = azureStore.accounts.find((a) => a.id === p.azure_account_id)
  if (azure) {
    out.push({
      key: 'azure', icon: CloudCog, tone: 'primary', label: azure.label, extra: p.azure_account_ids.length - 1,
      title: t('projects.azureChipTitle', { label: azure.label, subscription: azure.subscription_name ?? azure.subscription_id ?? '—' }),
    })
  }
  for (const s of servers.value) {
    const tone: Tone = s.status === 'online' ? 'success' : s.status === 'offline' ? 'error' : 'neutral'
    const status = s.status ? ` · ${s.status}` : ''
    out.push({
      key: `srv-${s.id}-${s.role}`, icon: HardDrive, tone, label: s.label, extra: 0,
      title: `${s.role}: ${s.username}@${s.host}:${s.port}${status}`,
    })
  }
  return out
})

// --- Actions ---
async function openWith(action: () => Promise<void>) {
  try {
    await action()
  } catch (e) {
    toast.error(String(e))
  }
}
const openInVscode = () => openWith(() => store.openInVscode(props.project.path))
const openInFolder = () => openWith(() => store.openInFolder(props.project.path))
const openInTerminal = () => openWith(async () => {
  const settings = await invoke<{ terminal_app: string }>('get_settings')
  await store.openInTerminal(props.project.path, settings.terminal_app)
})

async function toggleHidden() {
  const project = props.project
  const hidden = !project.hidden
  try {
    await store.updateAppearance(project, { hidden })
    // Its tab goes too; opening the project again brings the tab back.
    if (hidden) tabsStore.close(project.id)
    toast.success(t(hidden ? 'projects.toastHidden' : 'projects.toastShown', { name: project.name }))
  } catch (e) {
    toast.error(String(e))
  }
}

async function remove() {
  const project = props.project
  if (!(await confirm({
    title: t('projects.confirmRemoveTitle'),
    message: t('projects.confirmRemoveMessage', { name: project.name }),
    confirmLabel: t('common.remove'),
  }))) return
  try {
    await store.removeProject(project.id)
    toast.success(t('projects.toastDeleted'))
  } catch (e) {
    toast.error(String(e))
  }
}
</script>

<template>
  <div class="flex min-h-0 min-w-0 flex-1 flex-col">
    <DetailHeader :title="project.name">
      <template #leading>
        <slot name="leading" />
        <span
          class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border"
          :class="projectTileClass(project.color)"
        >
          <component :is="projectIcon(project.icon)" class="h-3.5 w-3.5" :stroke-width="1.75" />
        </span>
      </template>
      <template #meta>
        <span class="font-mono" :title="project.path">{{ project.path }}</span>
      </template>
      <template #actions>
        <Button variant="outline" size="icon" :title="t('projects.openInVscode')" :aria-label="t('projects.openInVscode')" @click="openInVscode">
          <Code2 class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
        <Button variant="outline" size="icon" :title="t('projects.openFolder')" :aria-label="t('projects.openFolder')" @click="openInFolder">
          <FolderOpen class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
        <Button variant="outline" size="icon" :title="t('projects.openInTerminal')" :aria-label="t('projects.openInTerminal')" @click="openInTerminal">
          <SquareTerminal class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
        <Button
          v-if="features.allows('projects.gantt')"
          variant="outline"
          size="icon"
          :title="t('projects.gantt')"
          :aria-label="t('projects.gantt')"
          @click="openGanttWindow(project.id)"
        >
          <GanttChartSquare class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
        <Button
          variant="outline"
          size="icon"
          :title="project.hidden ? t('projects.unhideProject') : t('projects.hideProject')"
          :aria-label="project.hidden ? t('projects.unhideProject') : t('projects.hideProject')"
          @click="toggleHidden"
        >
          <component :is="project.hidden ? Eye : EyeOff" class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
        <Button
          variant="outline"
          size="icon"
          :title="t('projects.projectSettings')"
          :aria-label="t('projects.projectSettings')"
          @click="router.push(`/projects/${project.id}/settings`)"
        >
          <Settings class="h-3.5 w-3.5" :stroke-width="1.75" />
        </Button>
      </template>
    </DetailHeader>

    <div class="flex shrink-0 border-b border-border/60 px-5 text-xs font-medium" role="tablist">
      <button
        v-for="id in PROJECT_TABS"
        :key="id"
        type="button"
        role="tab"
        :aria-selected="tab === id"
        class="cursor-pointer px-3 py-2 transition-colors"
        :class="tab === id ? 'border-b-2 border-primary text-foreground' : 'text-muted-foreground hover:text-foreground'"
        @click="tab = id"
      >
        {{ t(`projectDetail.groups.${id}`) }}
      </button>
    </div>

    <div v-if="tab === 'overview'" class="min-h-0 flex-1 overflow-y-auto px-5 pb-6">
      <div class="max-w-3xl">
        <!-- Numbers -->
        <dl class="grid grid-cols-2 gap-3 py-4 xl:grid-cols-4">
          <div class="rounded-lg border border-border bg-card px-3 py-2.5">
            <dt class="text-2xs text-muted-foreground">{{ t('projects.overview.sessions') }}</dt>
            <dd class="mt-0.5 text-sm font-semibold tabular-nums">{{ project.run_count }}</dd>
          </div>
          <div class="rounded-lg border border-border bg-card px-3 py-2.5">
            <dt class="text-2xs text-muted-foreground">{{ t('projects.overview.running') }}</dt>
            <dd class="mt-0.5 flex items-center gap-1.5 text-sm font-semibold tabular-nums">
              <span v-if="runningCount > 0" class="h-1.5 w-1.5 rounded-full bg-success" />
              {{ runningCount }}
            </dd>
          </div>
          <div class="rounded-lg border border-border bg-card px-3 py-2.5">
            <dt class="text-2xs text-muted-foreground">{{ t('projects.overview.lastSession') }}</dt>
            <dd
              class="mt-0.5 text-sm font-semibold tabular-nums"
              :title="lastSession ? formatRunTimestamp(lastSession) : undefined"
            >
              {{ lastSession ? relativeAge(lastSession, t('run.relNow')) : '—' }}
            </dd>
          </div>
          <div class="rounded-lg border border-border bg-card px-3 py-2.5">
            <dt class="text-2xs text-muted-foreground">{{ t('projects.repositories') }}</dt>
            <dd class="mt-0.5 text-sm font-semibold tabular-nums">{{ repos ? repos.length : '…' }}</dd>
          </div>
        </dl>

        <DetailSection :title="t('projectDetail.groups.defaults')">
          <template #actions>
            <Button variant="outline" size="xs" @click="router.push(settingsPath('session'))">{{ t('common.edit') }}</Button>
          </template>
          <dl class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-xs">
            <dt class="text-muted-foreground">{{ t('projects.overview.engine') }}</dt>
            <dd>{{ engineLabel }}</dd>
            <dt class="text-muted-foreground">{{ t('projects.overview.model') }}</dt>
            <dd class="min-w-0 truncate" :title="modelLabel">{{ modelLabel }}</dd>
            <template v-if="defaults.engine !== 'codex'">
              <dt class="text-muted-foreground">{{ t('projects.overview.claudeAccount') }}</dt>
              <dd class="min-w-0 truncate" :title="claudeAccountLabel">{{ claudeAccountLabel }}</dd>
            </template>
          </dl>
        </DetailSection>

        <DetailSection :title="t('projects.repositories')">
          <p v-if="repos && repos.length === 0" class="text-2xs text-muted-foreground">{{ t('projects.overview.noRepos') }}</p>
          <ul v-else-if="repos" class="space-y-1.5">
            <li v-for="repo in repos" :key="repo.id" class="flex items-center gap-2 rounded-md border border-border px-3 py-2">
              <component
                :is="repo.provider === 'gitlab' ? Gitlab : Github"
                class="h-3.5 w-3.5 shrink-0 text-muted-foreground"
                :stroke-width="1.75"
              />
              <div class="min-w-0 flex-1">
                <p class="truncate text-xs font-medium" :title="repo.name">{{ repo.name }}</p>
                <p class="truncate font-mono text-2xs text-muted-foreground" :title="repoCoords(repo)">{{ repoCoords(repo) }}</p>
              </div>
            </li>
          </ul>
        </DetailSection>

        <DetailSection :title="t('projectDetail.groups.connections')">
          <template #actions>
            <Button variant="outline" size="xs" @click="router.push(settingsPath('github'))">{{ t('common.edit') }}</Button>
          </template>
          <div v-if="connections.length" class="flex flex-wrap items-center gap-1.5">
            <Badge v-for="chip in connections" :key="chip.key" :tone="chip.tone" class="max-w-[12rem]" :title="chip.title">
              <component :is="chip.icon" class="h-2.5 w-2.5 shrink-0" :stroke-width="2" />
              <span class="truncate">{{ chip.label }}</span>
              <span v-if="chip.extra > 0" class="shrink-0 opacity-70">+{{ chip.extra }}</span>
            </Badge>
          </div>
          <p v-else class="text-2xs text-muted-foreground">{{ t('projects.overview.noConnections') }}</p>
        </DetailSection>

        <DetailSection :title="t('projects.removeProject')" :hint="t('projects.overview.removeHint')">
          <div>
            <Button variant="destructive" size="xs" @click="remove">
              <Trash2 class="h-3 w-3" :stroke-width="1.75" />
              {{ t('projects.removeProject') }}
            </Button>
          </div>
        </DetailSection>
      </div>
    </div>
  </div>
</template>
