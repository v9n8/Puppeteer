<script setup lang="ts">
// MCP section of the Connections tab: which MCP servers the project gets, built-in
// and the user's own, one switch each. Adding, editing and testing servers is done
// on the MCP screen (V3-44, QĐ-23).
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Server } from 'lucide-vue-next'
import { Badge, Banner, Card, ToggleSwitch } from '@/components/ui'
import { useToast } from '@/composables/useToast'
import { composerDefaults } from '@/lib/composerChips'
import { type Project } from '@/stores/projects'
import { useMcpServersStore, type ProjectBuiltinMcp, type ProjectMcpServer } from '@/stores/mcpServers'
import { useAppSettingsStore } from '@/stores/appSettings'
import ProjectSlackDm from '@/components/projects/ProjectSlackDm.vue'

/** The MCP management screen (Settings › Connections › MCP, V3-51). */
const MCP_ROUTE = '/settings/mcp'

const props = defineProps<{ project: Project }>()

const { t } = useI18n()
const { toast } = useToast()
const mcpStore = useMcpServersStore()
const appSettings = useAppSettingsStore()

const projectId = computed(() => props.project.id)

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

// Each load drops an answer for a project the user already left.
watch(projectId, async () => {
  await appSettings.ensureLoaded()
  await loadProjectMcpServers()
}, { immediate: true })
</script>

<template>
<div class="max-w-lg space-y-5">
  <Card>
    <template #header>
      <Server class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('projectDetail.mcpServers') }}</span>
      <span
        v-if="mcpServers.filter(s => s.enabled_for_project).length > 0"
        class="flex h-4 min-w-4 items-center justify-center rounded-full bg-muted px-1 text-2xs font-medium text-muted-foreground leading-none"
      >{{ mcpServers.filter(s => s.enabled_for_project).length }}</span>
      <RouterLink :to="MCP_ROUTE" class="ml-auto text-2xs text-primary hover:underline">{{ t('projectDetail.manage') }}</RouterLink>
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
      <h3 class="px-4 pt-3 pb-1 text-2xs font-medium uppercase tracking-wide text-muted-foreground">{{ t('projectDetail.mcpBuiltinGroup') }}</h3>
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
      <RouterLink :to="MCP_ROUTE" class="text-primary hover:underline">{{ t('projectDetail.createOne') }}</RouterLink>
      {{ t('projectDetail.toEnableHere') }}
    </div>

    <!-- Server list with on/off toggles -->
    <div v-else class="divide-y divide-border/50">
      <h3 class="px-4 pt-3 pb-1 text-2xs font-medium uppercase tracking-wide text-muted-foreground">{{ t('projectDetail.mcpOwnGroup') }}</h3>
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
  <ProjectSlackDm :project="project" />
</div>
</template>
