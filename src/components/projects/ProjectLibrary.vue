<script setup lang="ts">
// Library tab of the project detail column: what the project has on disk
// (skills, rules, commands, agents), read-only with links into the Library
// screen that edits them (UX-57); which MCP servers the project gets; and the
// old library's skill/rule sync conflicts to resolve.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertCircle, GitMerge, Library, Server } from 'lucide-vue-next'
import { Badge, Banner, Button, Card, ToggleSwitch } from '@/components/ui'
import { useToast } from '@/composables/useToast'
import { composerDefaults } from '@/lib/composerChips'
import { invoke } from '@/lib/tauri'
import { CMD_LIBRARY_SCAN, kindGroups, libraryQuery, type DiskItem } from '@/lib/libraryDisk'
import { useProjectsStore, type InstructionWarning, type Project } from '@/stores/projects'
import { useMcpServersStore, type ProjectBuiltinMcp, type ProjectMcpServer } from '@/stores/mcpServers'
import { useAppSettingsStore } from '@/stores/appSettings'

const props = defineProps<{ project: Project; section: 'files' | 'mcp' | 'conflicts' }>()

const { t } = useI18n()
const { toast } = useToast()
const projectStore = useProjectsStore()
const mcpStore = useMcpServersStore()
const appSettings = useAppSettingsStore()

const projectId = computed(() => props.project.id)

const projectConflicts = computed(() => projectStore.conflicts.filter(c => c.project_id === projectId.value))
const projectRuleConflicts = computed(() => projectStore.ruleConflicts.filter(c => c.project_id === projectId.value))

// --- Files on disk (the Library screen's project tab, listed only) ---
const files = ref<DiskItem[]>([])
const loadingFiles = ref(false)
const filesError = ref<string | null>(null)
// Kinds the project has none of are left out: the list stays short.
const fileGroups = computed(() =>
  kindGroups(files.value, '', k => t(`library.kind.${k}`)).filter(g => g.rows.length > 0),
)

async function loadFiles() {
  const id = projectId.value
  loadingFiles.value = true
  try {
    const list = await invoke<DiskItem[]>(CMD_LIBRARY_SCAN, { scope: 'project', projectId: id })
    if (id !== projectId.value) return
    files.value = list
    filesError.value = null
  } catch (e) {
    if (id !== projectId.value) return
    files.value = []
    filesError.value = String(e)
  } finally {
    loadingFiles.value = false
  }
}

/** The Library screen on this project's tab, at one file when given. */
function libraryLink(path: string | null = null) {
  return { path: '/library', query: libraryQuery({ scope: 'project', projectId: projectId.value, path }) }
}

// How Claude/Codex will load this project's instruction files — warnings only, never auto-fixed.
const instructionWarnings = ref<InstructionWarning[]>([])
async function loadInstructionWarnings() {
  const id = projectId.value
  const warnings = await projectStore.getInstructionWarnings(id).catch(() => [])
  if (id === projectId.value) instructionWarnings.value = warnings
}
function instructionWarningText(w: InstructionWarning): string {
  switch (w.code) {
    case 'agents_md_too_large':
      return t('projectDetail.warnAgentsMdTooLarge', { size: Math.ceil(w.size / 1024), limit: Math.floor(w.limit / 1024) })
    case 'claude_reads_agents_md':
      return t('projectDetail.warnClaudeReadsAgentsMd')
    case 'claude_md_imports_agents_md':
      return t('projectDetail.warnClaudeMdImportsAgentsMd', { path: w.path })
    case 'codex_reads_claude_md':
      return t('projectDetail.warnCodexReadsClaudeMd', { config: w.config })
    case 'links_not_ignored':
      return t('projectDetail.warnLinksNotIgnored', { dir: w.dir })
    case 'legacy_copies_backed_up':
      return t('projectDetail.warnLegacyCopiesBackedUp', { dir: w.dir, count: w.batches })
    case 'tracked_by_git':
      return t('projectDetail.warnTrackedByGit', { paths: w.paths.join(', ') })
  }
}

// --- MCP servers (per-project enable/disable) ---
const mcpServers = ref<ProjectMcpServer[]>([])
const loadingMcp = ref(false)
const savingMcp = ref(false)
const togglingMcpId = ref<string | null>(null)
// Warn (not block) when a legacy SSE server is enabled but the default engine is
// Codex, which supports stdio + streamable HTTP MCP. The project's default first (UX-19).
const defaultIsCodex = computed(
  () => composerDefaults({ settings: appSettings.settings, project: props.project.session_defaults }).engine === 'codex',
)

// Built-in servers (puppeteer, google): on by default, switched off per project.
const builtinMcp = ref<ProjectBuiltinMcp[]>([])
const togglingBuiltin = ref<string | null>(null)

async function loadProjectMcpServers() {
  const id = projectId.value
  loadingMcp.value = true
  try {
    const [servers, builtins] = await Promise.all([
      mcpStore.listForProject(id),
      mcpStore.listBuiltinForProject(id),
    ])
    if (id !== projectId.value) return
    mcpServers.value = servers
    builtinMcp.value = builtins
  } finally {
    loadingMcp.value = false
  }
}

async function handleToggleBuiltinMcp(item: ProjectBuiltinMcp) {
  const next = !item.enabled_for_project
  item.enabled_for_project = next
  togglingBuiltin.value = item.name
  try {
    await mcpStore.setBuiltinForProject(projectId.value, item.name, next)
    toast.success(t('projectDetail.toastSaved'))
  } catch (e) {
    item.enabled_for_project = !next
    toast.error(String(e))
  } finally {
    togglingBuiltin.value = null
  }
}

async function handleToggleMcpServer(server: ProjectMcpServer) {
  const next = !server.enabled_for_project
  server.enabled_for_project = next
  savingMcp.value = true
  togglingMcpId.value = server.id
  try {
    const ids = mcpServers.value.filter(s => s.enabled_for_project).map(s => s.id)
    await mcpStore.setForProject(projectId.value, ids)
    toast.success(t('projectDetail.toastSaved'))
  } catch (e) {
    server.enabled_for_project = !next
    toast.error(String(e))
  } finally {
    savingMcp.value = false
    togglingMcpId.value = null
  }
}

// The whole tab loads once per project, so moving between its sections reads nothing again.
// Each load drops an answer for a project the user already left.
watch(projectId, async () => {
  await appSettings.ensureLoaded()
  await loadFiles()
  await loadInstructionWarnings()
  await loadProjectMcpServers()
}, { immediate: true })
</script>

<template>
  <!-- Files on disk: listed here, edited on the Library screen -->
  <div v-if="section === 'files'" class="max-w-lg space-y-5">
    <Banner v-if="instructionWarnings.length > 0" tone="warning">
      <p class="font-medium text-warning">{{ t('projectDetail.instructionWarningsTitle') }}</p>
      <p
        v-for="(w, i) in instructionWarnings"
        :key="i"
        class="mt-1 leading-relaxed"
      >{{ instructionWarningText(w) }}</p>
    </Banner>
    <Card>
      <template #header>
        <Library class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('library.subtitle') }}</span>
        <span
          v-if="files.length > 0"
          class="flex h-4 min-w-4 items-center justify-center rounded-full bg-muted px-1 text-2xs font-medium text-muted-foreground leading-none"
        >{{ files.length }}</span>
        <RouterLink
          :to="libraryLink()"
          class="ml-auto shrink-0 text-2xs text-primary hover:underline"
          :title="t('projectDetail.openInLibraryHint')"
        >{{ t('projectDetail.openInLibrary') }}</RouterLink>
      </template>

      <!-- Loading -->
      <div v-if="loadingFiles" class="divide-y divide-border/50">
        <div v-for="i in 2" :key="i" class="flex items-center gap-3 px-4 py-3">
          <div class="flex-1 space-y-1.5">
            <div class="h-2.5 w-24 bg-muted animate-pulse rounded" />
            <div class="h-2 w-36 bg-muted animate-pulse rounded" />
          </div>
        </div>
      </div>

      <div v-else-if="filesError" class="px-4 py-6 text-center text-xs text-destructive break-words">{{ filesError }}</div>

      <div
        v-else-if="files.length === 0"
        class="px-4 py-6 text-center text-xs text-muted-foreground"
      >
        {{ t('projectDetail.libraryFilesEmpty') }}
      </div>

      <!-- One block per kind; a row opens that file in the Library -->
      <div v-else class="divide-y divide-border/60">
        <section v-for="group in fileGroups" :key="group.key">
          <h3 class="flex items-center gap-1.5 px-4 pt-3 pb-1 text-2xs font-medium uppercase tracking-wide text-muted-foreground">
            {{ group.label }}
            <span class="tabular-nums">{{ group.rows.length }}</span>
          </h3>
          <RouterLink
            v-for="row in group.rows"
            :key="row.id"
            :to="libraryLink(row.item.path)"
            class="block px-4 py-2 transition-colors hover:bg-accent/50"
            :title="t('projectDetail.openFileInLibrary', { name: row.item.name })"
          >
            <span class="flex min-w-0 items-center gap-1.5">
              <span class="min-w-0 truncate font-mono text-xs font-medium" :title="row.item.name">{{ row.item.name }}</span>
              <span v-if="row.item.error" class="shrink-0 text-destructive" :title="row.item.error">
                <AlertCircle class="h-3.5 w-3.5" :stroke-width="1.75" />
              </span>
              <Badge
                v-if="row.item.status !== 'file'"
                size="xs"
                :tone="row.item.status === 'puppeteer_link' ? 'warning' : 'neutral'"
                class="shrink-0"
                :title="t(`library.statusHint.${row.item.status}`)"
              >{{ t(`library.status.${row.item.status}`) }}</Badge>
              <Badge size="xs" class="shrink-0">{{ t(`library.engine.${row.item.engine}`) }}</Badge>
            </span>
            <span
              v-if="row.item.description"
              class="mt-0.5 block truncate text-2xs text-muted-foreground"
              :title="row.item.description"
            >{{ row.item.description }}</span>
          </RouterLink>
        </section>
      </div>
    </Card>
  </div>

  <!-- MCP servers -->
  <div v-else-if="section === 'mcp'" class="max-w-lg space-y-5">
    <Card>
      <template #header>
        <Server class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('projectDetail.mcpServers') }}</span>
        <span
          v-if="mcpServers.filter(s => s.enabled_for_project).length > 0"
          class="flex h-4 min-w-4 items-center justify-center rounded-full bg-muted px-1 text-2xs font-medium text-muted-foreground leading-none"
        >{{ mcpServers.filter(s => s.enabled_for_project).length }}</span>
        <RouterLink to="/mcp" class="ml-auto text-2xs text-primary hover:underline">{{ t('projectDetail.manage') }}</RouterLink>
      </template>

      <!-- Codex warning banner -->
      <Banner
        v-if="defaultIsCodex && mcpServers.some(s => s.transport === 'sse' && s.enabled_for_project)"
        tone="warning"
        class="mx-4 mt-4 leading-relaxed"
      >
        {{ t('projectDetail.codexWarning') }}
      </Banner>

      <!-- Built-in servers (on by default; off per project) -->
      <div v-if="!loadingMcp && builtinMcp.length" class="divide-y divide-border/50 border-b border-border/50">
        <div
          v-for="item in builtinMcp"
          :key="item.name"
          class="flex items-center gap-3 px-4 py-3"
        >
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-1.5 flex-wrap">
              <p class="text-xs font-medium font-mono truncate">{{ item.name }}</p>
              <Badge tone="neutral" size="xs" class="shrink-0 uppercase tracking-wide">{{ t('projectDetail.builtinMcp') }}</Badge>
              <Badge
                v-if="!item.enabled_globally"
                tone="neutral"
                size="xs"
                class="shrink-0"
                :title="t('projectDetail.builtinMcpOffGloballyTitle')"
              >{{ t('projectDetail.disabled') }}</Badge>
            </div>
            <p class="text-2xs text-muted-foreground truncate mt-0.5">mcp__{{ item.name }}__*</p>
          </div>
          <ToggleSwitch
            :checked="item.enabled_for_project"
            :disabled="togglingBuiltin !== null"
            :loading="togglingBuiltin === item.name"
            :label="item.enabled_for_project ? t('projectDetail.disableForProject') : t('projectDetail.enableForProject')"
            @toggle="handleToggleBuiltinMcp(item)"
          />
        </div>
      </div>

      <!-- Loading -->
      <div v-if="loadingMcp" class="divide-y divide-border/50">
        <div v-for="i in 2" :key="i" class="flex items-center gap-3 px-4 py-3">
          <div class="flex-1 space-y-1.5">
            <div class="h-2.5 w-24 bg-muted animate-pulse rounded" />
            <div class="h-2 w-36 bg-muted animate-pulse rounded" />
          </div>
          <div class="h-6 w-11 rounded-full bg-muted animate-pulse shrink-0" />
        </div>
      </div>

      <!-- Empty -->
      <div
        v-else-if="mcpServers.length === 0"
        class="px-4 py-6 text-center text-xs text-muted-foreground"
      >
        {{ t('projectDetail.noMcpPrefix') }}
        <RouterLink to="/mcp" class="text-primary hover:underline">{{ t('projectDetail.createOne') }}</RouterLink>
        {{ t('projectDetail.toEnableHere') }}
      </div>

      <!-- Server list with on/off toggles -->
      <div v-else class="divide-y divide-border/50">
        <div
          v-for="server in mcpServers"
          :key="server.id"
          class="flex items-center gap-3 px-4 py-3"
        >
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-1.5 flex-wrap">
              <p class="text-xs font-medium font-mono truncate">{{ server.name }}</p>
              <Badge tone="neutral" size="xs" class="shrink-0 uppercase tracking-wide">{{ server.transport }}</Badge>
              <Badge v-if="!server.enabled" tone="neutral" size="xs" class="shrink-0">{{ t('projectDetail.disabled') }}</Badge>
              <Badge
                v-if="server.transport === 'sse' && defaultIsCodex"
                tone="warning"
                size="xs"
                class="shrink-0"
                :title="t('projectDetail.claudeOnlyTitle')"
              >{{ t('projectDetail.claudeOnly') }}</Badge>
            </div>
            <p v-if="server.description" class="text-2xs text-muted-foreground truncate mt-0.5">{{ server.description }}</p>
          </div>
          <ToggleSwitch
            :checked="server.enabled_for_project"
            :disabled="savingMcp"
            :loading="togglingMcpId === server.id"
            :label="server.enabled_for_project ? t('projectDetail.disableForProject') : t('projectDetail.enableForProject')"
            @toggle="handleToggleMcpServer(server)"
          />
        </div>
      </div>
    </Card>
  </div>

  <!-- Conflicts -->
  <div v-else class="max-w-lg space-y-3">
    <!-- Skill conflicts -->
    <Card
      v-for="conflict in projectConflicts"
      :key="conflict.id"
      class="border-warning/20"
      bodyClass="p-4"
    >
      <div class="flex items-start gap-3 mb-3">
        <GitMerge class="h-4 w-4 text-warning mt-0.5 shrink-0" :stroke-width="1.75" />
        <div>
          <div class="flex items-center gap-1.5">
            <p class="text-sm font-mono font-medium">{{ conflict.skill_name }}</p>
            <Badge tone="neutral" size="xs" class="uppercase tracking-wide">
              {{ t('projectDetail.conflictSkillBadge', { path: conflict.engine === 'codex' ? '.codex/skills' : '.claude/skills' }) }}
            </Badge>
          </div>
          <p class="text-xs text-muted-foreground mt-0.5">
            {{ t('projectDetail.localChangesDetected', { date: new Date(conflict.detected_at).toLocaleDateString() }) }}
          </p>
        </div>
      </div>
      <div class="flex gap-2">
        <Button class="flex-1" @click="projectStore.resolveConflict(conflict.id, true); projectStore.fetchConflicts()">
          {{ t('projectDetail.overwriteWithCentral') }}
        </Button>
        <Button variant="outline" class="flex-1" @click="projectStore.resolveConflict(conflict.id, false); projectStore.fetchConflicts()">
          {{ t('projectDetail.keepLocal') }}
        </Button>
      </div>
    </Card>

    <!-- Rule conflicts -->
    <Card
      v-for="conflict in projectRuleConflicts"
      :key="conflict.id"
      class="border-warning/20"
      bodyClass="p-4"
    >
      <div class="flex items-start gap-3 mb-3">
        <GitMerge class="h-4 w-4 text-warning mt-0.5 shrink-0" :stroke-width="1.75" />
        <div>
          <div class="flex items-center gap-1.5">
            <p class="text-sm font-mono font-medium">{{ conflict.rule_name }}</p>
            <Badge tone="neutral" size="xs" class="uppercase tracking-wide">
              {{ t('projectDetail.conflictRuleBadge', { path: conflict.engine === 'claude' ? '.claude/rules' : 'AGENTS.md' }) }}
            </Badge>
          </div>
          <p class="text-xs text-muted-foreground mt-0.5">
            {{ t('projectDetail.localChangesDetected', { date: new Date(conflict.detected_at).toLocaleDateString() }) }}
          </p>
        </div>
      </div>
      <div class="flex gap-2">
        <Button class="flex-1" @click="projectStore.resolveRuleConflict(conflict.id, true)">
          {{ t('projectDetail.overwriteWithCentral') }}
        </Button>
        <Button variant="outline" class="flex-1" @click="projectStore.resolveRuleConflict(conflict.id, false)">
          {{ t('projectDetail.keepLocal') }}
        </Button>
      </div>
    </Card>
  </div>
</template>
