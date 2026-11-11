<script setup lang="ts">
// Overview section of the project detail column: numbers, the General fields
// (name, colour, icon, path), a summary of session defaults and connections,
// the repos, and removal. Nothing here opens a session (D3).
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { openUrl } from '@tauri-apps/plugin-opener'
import {
  AlertTriangle, Cloud, CloudCog, ExternalLink, Github, Gitlab, GitBranch, HardDrive, Plus, Trash2,
} from 'lucide-vue-next'
import { Badge, Button, Input } from '@/components/ui'
import DetailSection from '@/components/listDetail/DetailSection.vue'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { composerDefaults } from '@/lib/composerChips'
import {
  PROJECT_COLORS, PROJECT_ICONS, projectColor, projectIcon, swatchClass,
  type ProjectColor, type ProjectIcon,
} from '@/lib/projectAppearance'
import type { ProjectSectionId } from '@/lib/projectSections'
import { parseRepoUrl, repoKey, repoWebUrl } from '@/lib/repoUrl'
import { formatRunTimestamp, relativeAge } from '@/lib/runDisplay'
import { useProjectsStore, type Project, type Repo } from '@/stores/projects'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useAwsAccountsStore } from '@/stores/awsAccounts'
import { useAzureAccountsStore } from '@/stores/azureAccounts'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { useGithubAccountsStore } from '@/stores/githubAccounts'
import { useGitlabAccountsStore } from '@/stores/gitlabAccounts'
import { useLiveRunsStore } from '@/stores/liveRuns'
import { useServersStore, type ProjectServer } from '@/stores/servers'

const props = defineProps<{ project: Project }>()
const emit = defineEmits<{ open: [section: ProjectSectionId] }>()

const { t } = useI18n()
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
const live = useLiveRunsStore()

appSettings.ensureLoaded().catch(() => {})
if (!claudeStore.accounts.length) claudeStore.fetch().catch(() => {})

// --- Repos and mapped servers (no bulk endpoint: loaded per selected project) ---
const repos = ref<Repo[] | null>(null)
const servers = ref<ProjectServer[]>([])

async function loadRepos(id = props.project.id) {
  const r = await store.listRepos(id).catch(() => [] as Repo[])
  // A slower answer for a project the user already left is dropped.
  if (props.project.id === id) repos.value = r
}

watch(() => props.project.id, async (id) => {
  repos.value = null
  servers.value = []
  const [, s] = await Promise.all([
    loadRepos(id),
    serversStore.listForProject(id).catch(() => [] as ProjectServer[]),
  ])
  if (props.project.id === id) servers.value = s
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

// --- General: the name saves 500 ms after the last keystroke; colour and icon on click ---
const editName = ref(props.project.name)
let pendingSave: { timer: ReturnType<typeof setTimeout>; run: () => void } | null = null

/** Run a waiting name save now, so leaving the project or the page does not drop it. */
function flushSave() {
  if (!pendingSave) return
  clearTimeout(pendingSave.timer)
  const { run } = pendingSave
  pendingSave = null
  run()
}

async function saveName(id: string, name: string) {
  const project = store.projects.find((p) => p.id === id)
  if (!project || !name.trim() || name === project.name) return
  try {
    await store.updateProject({
      id,
      name,
      github_project_board_url: project.github_project_board_url,
      github_project_field_mappings: project.github_project_field_mappings,
    })
    toast.success(t('projectDetail.toastSaved'))
  } catch (e) {
    toast.error(String(e))
  }
}

// Bound to typing only, so filling the field from the project never writes.
function onNameInput(name: string) {
  editName.value = name
  if (pendingSave) clearTimeout(pendingSave.timer)
  const id = props.project.id
  const run = () => { void saveName(id, name) }
  pendingSave = { timer: setTimeout(() => { pendingSave = null; run() }, 500), run }
}

watch(() => props.project.id, () => {
  flushSave()
  editName.value = props.project.name
})
watch(() => props.project.name, (name) => {
  if (!pendingSave) editName.value = name
})
onBeforeUnmount(flushSave)

// Picking the current colour again returns to the default look.
async function setAppearance(patch: { color?: ProjectColor | null; icon?: ProjectIcon | null }) {
  try {
    await store.updateAppearance(props.project, patch)
    toast.success(t('projectDetail.toastSaved'))
  } catch (e) {
    toast.error(String(e))
  }
}

function pickColor(color: ProjectColor) {
  setAppearance({ color: projectColor(props.project.color) === color ? null : color })
}

function pickIcon(icon: ProjectIcon | null) {
  if ((props.project.icon ?? null) === icon) return
  setAppearance({ icon })
}

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

// --- Repos: paste a URL to add, remove to change ---
const newRepoUrl = ref('')
const addingRepo = ref(false)
watch(() => props.project.id, () => { newRepoUrl.value = '' })
const parsedRepo = computed(() => parseRepoUrl(newRepoUrl.value))

// The repo being entered already exists in this project.
const isDuplicateRepo = computed(() => {
  const parsed = parsedRepo.value
  if (!parsed) return false
  const key = repoKey(parsed)
  return (repos.value ?? []).some((r) => repoKey(r) === key)
})
const canAddRepo = computed(() => !!parsedRepo.value && !isDuplicateRepo.value)
// URL typed but not yet a valid repo link — used to show a gentle hint.
const repoUrlInvalid = computed(() => !!newRepoUrl.value.trim() && !parsedRepo.value)

async function handleAddRepo() {
  const parsed = parsedRepo.value
  if (!parsed || isDuplicateRepo.value) {
    if (isDuplicateRepo.value) toast.error(t('projectDetail.toastRepoExists'))
    return
  }
  const id = props.project.id
  addingRepo.value = true
  try {
    const isGitlab = parsed.provider === 'gitlab'
    await store.addRepo({
      project_id: id,
      name: parsed.name.trim(),
      path: props.project.path,
      provider: parsed.provider,
      github_owner: isGitlab ? undefined : parsed.github_owner || undefined,
      github_repo: isGitlab ? undefined : parsed.github_repo || undefined,
      gitlab_project_path: isGitlab ? parsed.gitlab_project_path || null : null,
      gitlab_project_id: null,
    })
    newRepoUrl.value = ''
    await loadRepos(id)
    toast.success(t('projectDetail.toastRepoAdded'))
  } catch (e) {
    toast.error(String(e))
  } finally {
    addingRepo.value = false
  }
}

async function handleRemoveRepo(repoId: string) {
  if (!(await confirm({
    title: t('projectDetail.confirmRemoveRepoTitle'),
    message: t('projectDetail.confirmRemoveRepoMessage'),
    confirmLabel: t('common.remove'),
  }))) return
  try {
    await store.removeRepo(repoId)
    await loadRepos()
    toast.success(t('projectDetail.toastRepoRemoved'))
  } catch (e) {
    toast.error(String(e))
  }
}

function handleOpenRepo(repo: Repo) {
  const url = repoWebUrl(repo)
  if (!url) return
  openUrl(url).catch(() => { /* opener unavailable */ })
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

    <DetailSection :title="t('projectDetail.general')">
      <div class="max-w-lg space-y-4">
        <div class="space-y-1.5">
          <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('projectDetail.projectNameLabel') }}</label>
          <Input :model-value="editName" size="sm" @update:model-value="onNameInput" />
        </div>
        <div class="space-y-1.5">
          <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('projectDetail.colorLabel') }}</label>
          <div class="flex items-center gap-1.5">
            <button
              v-for="color in PROJECT_COLORS"
              :key="color"
              type="button"
              class="h-5 w-5 rounded-full border-2 transition-transform cursor-pointer"
              :class="[
                projectColor(project.color) === color ? 'border-foreground scale-110' : 'border-transparent',
                swatchClass(color),
              ]"
              :title="t(`projectDetail.colors.${color}`)"
              :aria-label="t(`projectDetail.colors.${color}`)"
              :aria-pressed="projectColor(project.color) === color"
              @click="pickColor(color)"
            />
            <span class="text-2xs text-muted-foreground ml-1">{{ t('projectDetail.colorHint') }}</span>
          </div>
        </div>
        <div class="space-y-1.5">
          <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('projectDetail.iconLabel') }}</label>
          <div class="flex flex-wrap items-center gap-1">
            <button
              v-for="icon in [null, ...PROJECT_ICONS]"
              :key="icon ?? 'default'"
              type="button"
              class="flex h-7 w-7 items-center justify-center rounded-md border transition-colors cursor-pointer"
              :class="(project.icon ?? null) === icon
                ? 'border-primary bg-primary/10 text-primary'
                : 'border-transparent text-muted-foreground hover:bg-accent hover:text-foreground'"
              :title="t(`projectDetail.icons.${icon ?? 'default'}`)"
              :aria-label="t(`projectDetail.icons.${icon ?? 'default'}`)"
              :aria-pressed="(project.icon ?? null) === icon"
              @click="pickIcon(icon)"
            >
              <component :is="projectIcon(icon)" class="h-3.5 w-3.5" :stroke-width="1.75" />
            </button>
          </div>
        </div>
        <div class="space-y-1.5">
          <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('projectDetail.pathLabel') }}</label>
          <p class="text-2xs text-muted-foreground font-mono break-all">{{ project.path }}</p>
        </div>
      </div>
    </DetailSection>

    <DetailSection :title="t('projectDetail.groups.defaults')">
      <template #actions>
        <Button variant="outline" size="xs" @click="emit('open', 'session')">{{ t('common.edit') }}</Button>
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
      <div class="max-w-lg space-y-3">
        <!-- Add new repo — paste a URL, no manual fields -->
        <div class="space-y-2">
          <div class="flex gap-2">
            <div class="relative flex-1">
              <GitBranch class="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground pointer-events-none" :stroke-width="1.5" />
              <Input
                v-model="newRepoUrl"
                type="text"
                size="sm"
                class="pl-8"
                :placeholder="t('projectDetail.repoUrlPlaceholder')"
                @keydown.enter="canAddRepo && handleAddRepo()"
              />
            </div>
            <Button size="sm" :disabled="addingRepo || !canAddRepo" @click="handleAddRepo">
              <Plus class="h-3.5 w-3.5" :stroke-width="2" />
              {{ addingRepo ? t('projectDetail.adding') : t('common.add') }}
            </Button>
          </div>

          <!-- Parsed preview: what will be added -->
          <div v-if="parsedRepo" class="flex items-center gap-1.5 flex-wrap text-2xs px-0.5">
            <Badge :tone="parsedRepo.provider === 'gitlab' ? 'warning' : 'info'" size="xs">
              <component :is="parsedRepo.provider === 'gitlab' ? Gitlab : Github" class="h-2.5 w-2.5" :stroke-width="2" />
              {{ parsedRepo.provider === 'gitlab' ? 'GitLab' : 'GitHub' }}
            </Badge>
            <span class="font-mono text-muted-foreground truncate">
              {{ parsedRepo.provider === 'gitlab' ? parsedRepo.gitlab_project_path : `${parsedRepo.github_owner}/${parsedRepo.github_repo}` }}
            </span>
            <span v-if="isDuplicateRepo" class="flex items-center gap-1 text-warning font-medium">
              <AlertTriangle class="h-2.5 w-2.5" :stroke-width="2" />
              {{ t('projectDetail.alreadyAdded') }}
            </span>
          </div>

          <!-- Invalid URL hint -->
          <p v-else-if="repoUrlInvalid" class="text-2xs text-muted-foreground px-0.5">
            {{ t('projectDetail.repoUrlHint') }}
          </p>
        </div>

        <div v-if="!repos" class="space-y-2">
          <div v-for="i in 2" :key="i" class="h-12 rounded-md bg-muted animate-pulse" />
        </div>
        <p v-else-if="repos.length === 0" class="text-2xs text-muted-foreground">{{ t('projects.overview.noRepos') }}</p>
        <ul v-else class="space-y-1.5">
          <li v-for="repo in repos" :key="repo.id" class="flex items-center gap-2 rounded-md border border-border px-3 py-2">
            <component
              :is="repo.provider === 'gitlab' ? Gitlab : Github"
              class="h-3.5 w-3.5 shrink-0 text-muted-foreground"
              :stroke-width="1.75"
            />
            <div class="min-w-0 flex-1">
              <button
                v-if="repoWebUrl(repo)"
                type="button"
                class="group/repo flex items-center gap-1 max-w-full text-xs font-medium truncate text-left hover:text-primary transition-colors cursor-pointer"
                :title="t('projectDetail.openRepo', { url: repoWebUrl(repo) })"
                @click="handleOpenRepo(repo)"
              >
                <span class="truncate group-hover/repo:underline">{{ repo.name }}</span>
                <ExternalLink class="h-3 w-3 shrink-0 opacity-0 group-hover/repo:opacity-100 transition-opacity" :stroke-width="2" />
              </button>
              <p v-else class="truncate text-xs font-medium" :title="repo.name">{{ repo.name }}</p>
              <p class="truncate font-mono text-2xs text-muted-foreground" :title="repoCoords(repo)">{{ repoCoords(repo) }}</p>
            </div>
            <Button
              variant="destructive-ghost"
              size="icon-sm"
              class="shrink-0"
              :title="t('projectDetail.removeRepo')"
              @click="handleRemoveRepo(repo.id)"
            >
              <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
            </Button>
          </li>
        </ul>
      </div>
    </DetailSection>

    <DetailSection :title="t('projectDetail.groups.connections')">
      <template #actions>
        <Button variant="outline" size="xs" @click="emit('open', 'github')">{{ t('common.edit') }}</Button>
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
</template>
