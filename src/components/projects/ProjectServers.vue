<script setup lang="ts">
// Servers & permissions tab of the project detail column: the VPS deploy
// targets switched on for the project, and its always-allow / always-deny tools.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { CheckCircle2, Rocket, ShieldCheck, Trash2, XCircle } from 'lucide-vue-next'
import { Badge, Button, Card, ToggleSwitch } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import type { Project } from '@/stores/projects'
import { useServersStore, type ProjectServer, type VpsServer } from '@/stores/servers'
import { useToolPermissionsStore } from '@/stores/toolPermissions'
import { useLiveRunsStore } from '@/stores/liveRuns'

const props = defineProps<{ project: Project; section: 'deploy' | 'tools' }>()

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()
const serversStore = useServersStore()
const toolPerms = useToolPermissionsStore()
const live = useLiveRunsStore()

const projectId = computed(() => props.project.id)

// ── Tool permissions (allow/deny always) for this project ──────────────────
const allowTools = computed(() => toolPerms.getAllow(projectId.value))
const denyTools = computed(() => toolPerms.getDeny(projectId.value))
const hasToolPerms = computed(() => allowTools.value.length > 0 || denyTools.value.length > 0)

function revokeTool(tool: string) {
  toolPerms.reset(projectId.value, tool)
  live.syncToolPermissions(projectId.value)
  toast.success(t('projectDetail.toastToolRevoked', { tool }))
}

function flipTool(tool: string, to: 'allow' | 'deny') {
  if (to === 'allow') toolPerms.allow(projectId.value, tool)
  else toolPerms.deny(projectId.value, tool)
  live.syncToolPermissions(projectId.value)
  toast.success(t('projectDetail.toastToolFlipped', { tool, to }))
}

async function resetAllToolPerms() {
  if (!(await confirm({
    title: t('projectDetail.resetToolPermsTitle'),
    message: t('projectDetail.resetToolPermsMessage'),
    confirmLabel: t('projectDetail.resetAll'),
  }))) return
  for (const tool of [...allowTools.value, ...denyTools.value]) toolPerms.reset(projectId.value, tool)
  live.syncToolPermissions(projectId.value)
  toast.success(t('projectDetail.toastToolPermsReset'))
}

// --- Deploy: per-project VPS enable/disable (simple on/off, no roles) ---
const projectServers = ref<ProjectServer[]>([])
const loadingDeploy = ref(false)
const togglingServerId = ref<string | null>(null)

// Set of VPS ids currently enabled (mapped) for this project.
const enabledServerIds = computed(() => new Set(projectServers.value.map(s => s.id)))

async function loadProjectServers() {
  const id = projectId.value
  loadingDeploy.value = true
  try {
    const servers = await serversStore.listForProject(id)
    // A slower answer for a project the user already left is dropped.
    if (id === projectId.value) projectServers.value = servers
  } finally {
    loadingDeploy.value = false
  }
}

// Toggle a VPS on/off for this project. Enabling maps it with the default role;
// disabling removes every role mapping the server has for the project.
async function handleToggleDeployServer(server: VpsServer) {
  if (togglingServerId.value) return
  togglingServerId.value = server.id
  const isEnabled = enabledServerIds.value.has(server.id)
  try {
    if (isEnabled) {
      const mappings = projectServers.value.filter(s => s.id === server.id)
      for (const m of mappings) {
        await serversStore.unmap(projectId.value, m.id, m.role)
      }
    } else {
      await serversStore.mapToProject(projectId.value, server.id, '')
    }
    await loadProjectServers()
    toast.success(t('projectDetail.toastSaved'))
  } catch (e) {
    toast.error(String(e))
  } finally {
    togglingServerId.value = null
  }
}

serversStore.fetchServers().catch(() => {})
watch(projectId, loadProjectServers, { immediate: true })
</script>

<template>
  <!-- Deploy targets (per-project VPS enable/disable) -->
  <div v-if="section === 'deploy'" class="max-w-lg space-y-5">
    <Card>
      <template #header>
        <Rocket class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('projectDetail.deployTargets') }}</span>
        <span
          v-if="projectServers.length > 0"
          class="flex h-4 min-w-4 items-center justify-center rounded-full bg-muted px-1 text-2xs font-medium text-muted-foreground leading-none"
        >{{ projectServers.length }}</span>
        <RouterLink to="/servers" class="ml-auto text-2xs text-primary hover:underline">{{ t('projectDetail.manage') }}</RouterLink>
      </template>

      <!-- Loading -->
      <div v-if="loadingDeploy" class="divide-y divide-border/50">
        <div v-for="i in 2" :key="i" class="flex items-center gap-3 px-4 py-3">
          <div class="flex-1 space-y-1.5">
            <div class="h-2.5 w-24 bg-muted animate-pulse rounded" />
            <div class="h-2 w-36 bg-muted animate-pulse rounded" />
          </div>
          <div class="h-6 w-11 rounded-full bg-muted animate-pulse shrink-0" />
        </div>
      </div>

      <!-- No VPS defined -->
      <div
        v-else-if="serversStore.items.length === 0"
        class="px-4 py-6 text-center text-xs text-muted-foreground"
      >
        {{ t('projectDetail.noVpsPrefix') }}
        <RouterLink to="/servers" class="text-primary hover:underline">{{ t('projectDetail.addOne') }}</RouterLink>
        {{ t('projectDetail.toEnableDeployments') }}
      </div>

      <!-- VPS list with on/off toggles -->
      <div v-else class="divide-y divide-border/50">
        <div
          v-for="server in serversStore.items"
          :key="server.id"
          class="flex items-center gap-3 px-4 py-3"
        >
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-1.5 flex-wrap">
              <p class="text-xs font-medium truncate">{{ server.label }}</p>
              <Badge
                v-if="enabledServerIds.has(server.id)"
                tone="primary"
                size="xs"
                class="shrink-0 uppercase tracking-wide"
              >{{ t('projectDetail.enabled') }}</Badge>
              <Badge v-if="server.has_passphrase" tone="neutral" size="xs" class="shrink-0">{{ t('projectDetail.passphrase') }}</Badge>
            </div>
            <p class="text-2xs text-muted-foreground font-mono truncate mt-0.5">
              {{ server.username }}@{{ server.host }}:{{ server.port }}
            </p>
          </div>
          <ToggleSwitch
            :checked="enabledServerIds.has(server.id)"
            :disabled="togglingServerId !== null"
            :loading="togglingServerId === server.id"
            :label="enabledServerIds.has(server.id) ? t('projectDetail.disableForProject') : t('projectDetail.enableForProject')"
            @toggle="handleToggleDeployServer(server)"
          />
        </div>
      </div>
    </Card>
  </div>

  <!-- Tool permissions -->
  <div v-else class="max-w-lg space-y-5">
    <Card body-class="divide-y divide-border/50">
      <template #header>
        <ShieldCheck class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('projectDetail.toolPermissions') }}</span>
        <Button
          v-if="hasToolPerms"
          variant="destructive-ghost"
          size="xs"
          class="ml-auto"
          @click="resetAllToolPerms"
        >{{ t('projectDetail.resetAll') }}</Button>
      </template>

      <p class="px-4 py-3 text-2xs text-muted-foreground leading-relaxed">
        {{ t('projectDetail.toolPermsIntro') }}
      </p>

      <!-- Allow always -->
      <div class="px-4 py-3">
        <div class="flex items-center gap-2 mb-2">
          <CheckCircle2 class="h-3.5 w-3.5 text-success" :stroke-width="1.75" />
          <span class="text-2xs font-semibold">{{ t('projectDetail.allowAlways') }}</span>
          <span class="text-2xs text-muted-foreground tabular-nums">{{ allowTools.length }}</span>
        </div>
        <div v-if="allowTools.length" class="space-y-1.5">
          <div
            v-for="tool in allowTools"
            :key="'allow-' + tool"
            class="flex items-center gap-2 rounded-md border border-border px-3 py-2"
          >
            <p class="flex-1 min-w-0 text-xs font-mono truncate" :title="tool">{{ tool }}</p>
            <Badge tone="success" size="xs" class="shrink-0">{{ t('projectDetail.allow') }}</Badge>
            <Button variant="ghost" size="xs" class="shrink-0" :title="t('projectDetail.flipToDeny')" @click="flipTool(tool, 'deny')">→ {{ t('projectDetail.deny') }}</Button>
            <Button variant="destructive-ghost" size="icon-sm" class="shrink-0" :title="t('projectDetail.revoke')" @click="revokeTool(tool)">
              <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
            </Button>
          </div>
        </div>
        <p v-else class="text-2xs text-muted-foreground">{{ t('projectDetail.noTools') }}</p>
      </div>

      <!-- Deny always -->
      <div class="px-4 py-3">
        <div class="flex items-center gap-2 mb-2">
          <XCircle class="h-3.5 w-3.5 text-destructive" :stroke-width="1.75" />
          <span class="text-2xs font-semibold">{{ t('projectDetail.denyAlways') }}</span>
          <span class="text-2xs text-muted-foreground tabular-nums">{{ denyTools.length }}</span>
        </div>
        <div v-if="denyTools.length" class="space-y-1.5">
          <div
            v-for="tool in denyTools"
            :key="'deny-' + tool"
            class="flex items-center gap-2 rounded-md border border-border px-3 py-2"
          >
            <p class="flex-1 min-w-0 text-xs font-mono truncate" :title="tool">{{ tool }}</p>
            <Badge tone="error" size="xs" class="shrink-0">{{ t('projectDetail.deny') }}</Badge>
            <Button variant="ghost" size="xs" class="shrink-0" :title="t('projectDetail.flipToAllow')" @click="flipTool(tool, 'allow')">→ {{ t('projectDetail.allow') }}</Button>
            <Button variant="destructive-ghost" size="icon-sm" class="shrink-0" :title="t('projectDetail.revoke')" @click="revokeTool(tool)">
              <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
            </Button>
          </div>
        </div>
        <p v-else class="text-2xs text-muted-foreground">{{ t('projectDetail.noTools') }}</p>
      </div>
    </Card>
  </div>
</template>
