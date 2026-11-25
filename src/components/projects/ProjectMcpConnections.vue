<script setup lang="ts">
// "Project and global connections" card of a project's MCP tab (FR-3): every MCP
// server a run in this project could load, found outside Puppeteer — the
// project's own sources (local scope, .mcp.json, project Codex config, other
// apps' project files) under "Project", then the global ones (with the claude.ai
// connectors per Claude profile) under "Global" — with the same rows, actions
// and labels as Settings' "Global connections", a blocked count and a reload.
// Project sources show only here, never in Settings (Q9).
import { computed, onUnmounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { Globe, RotateCw } from 'lucide-vue-next'
import { Banner, Button, Card } from '@/components/ui'
import McpDiscoveryRows from '@/components/settings/McpDiscoveryRows.vue'
import { blockedCountOf, projectSections } from '@/lib/mcpDiscovery'
import { settingsLocation } from '@/lib/settingsSections'
import { useMcpDiscoveryStore } from '@/stores/mcpDiscovery'

const props = defineProps<{ projectId: string }>()
const emit = defineEmits<{
  /** An import or sync changed the MCP list: the project's server list reloads. */
  changed: []
}>()

const { t } = useI18n()
const router = useRouter()
const store = useMcpDiscoveryStore()

const view = computed(() => store.viewOf(props.projectId))
const loading = computed(() => store.loading[props.projectId] ?? false)
const error = computed(() => store.errors[props.projectId] ?? null)
const sections = computed(() => (view.value ? projectSections(view.value) : []))

// Follow the shown project: stop following the one the user left.
watch(
  () => props.projectId,
  (id, old) => {
    if (old) store.unwatch(old)
    void store.watch(id)
  },
  { immediate: true },
)
onUnmounted(() => store.unwatch(props.projectId))

/** The managed copy lives in the MCP list (Settings). */
function openServer(serverId: string) {
  router.push(settingsLocation({ section: 'mcp' }, { id: serverId })).catch(() => {})
}
</script>

<template>
  <Card>
    <template #header>
      <Globe class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('mcp.discovery.projectTitle') }}</span>
      <span
        v-if="view && view.blockedCount > 0"
        class="flex h-4 min-w-4 items-center justify-center rounded-full bg-warning/15 px-1 text-2xs font-semibold leading-none text-warning"
        :title="t('mcp.discovery.blockedTitle')"
      >{{ view.blockedCount }}</span>
      <span v-if="view?.probing" class="truncate text-2xs text-muted-foreground">{{ t('mcp.discovery.probing') }}</span>
      <Button
        variant="ghost"
        size="xs"
        class="ml-auto shrink-0"
        :disabled="loading"
        :title="t('mcp.discovery.reloadTitle')"
        @click="store.reload(projectId)"
      >
        <RotateCw class="h-3 w-3" :class="{ 'animate-spin': loading || view?.probing }" :stroke-width="2" />
        {{ t('mcp.discovery.reload') }}
      </Button>
    </template>

    <p class="px-4 pt-3 pb-1 text-2xs leading-relaxed text-muted-foreground">{{ t('mcp.discovery.projectHint') }}</p>
    <Banner v-if="error" tone="destructive" class="mx-4 my-2">{{ t('mcp.discovery.loadFailed', { error }) }}</Banner>

    <div v-if="!view && loading" class="space-y-1.5 px-4 py-3">
      <div v-for="i in 2" :key="i" class="h-8 animate-pulse rounded bg-muted" />
    </div>
    <div v-for="section in sections" v-else :key="section.key" class="border-t border-border/50">
      <h3 class="flex items-center gap-1.5 px-4 pt-3 pb-1 text-2xs font-medium uppercase tracking-wide text-muted-foreground">
        {{ section.key === 'project' ? t('mcp.discovery.groupProject') : t('mcp.discovery.groupGlobal') }}
        <span class="font-normal normal-case tracking-normal">· {{ t('mcp.discovery.blockedCount', { count: blockedCountOf(section.rows) }) }}</span>
      </h3>
      <McpDiscoveryRows
        :rows="section.rows"
        :profiles="section.key === 'global' && view ? view.profiles : []"
        :problems="section.problems"
        :probing="view?.probing ?? false"
        @open="openServer"
        @changed="emit('changed')"
      />
    </div>
  </Card>
</template>
