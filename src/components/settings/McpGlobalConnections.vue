<script setup lang="ts">
// Right pane of Settings › Connections › MCP for the pinned "Global connections"
// entry (FR-2): every global MCP server found outside Puppeteer — Claude and
// Codex configs, claude.ai connectors per Claude profile, plugin and managed
// servers, other apps — always listed, with a reload button. Project-scoped
// sources show only under their project (Q9).
import { computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { RotateCw } from 'lucide-vue-next'
import { Banner, Button } from '@/components/ui'
import DetailHeader from '@/components/listDetail/DetailHeader.vue'
import McpDiscoveryRows from '@/components/settings/McpDiscoveryRows.vue'
import { clockText } from '@/lib/projectHealth'
import { useMcpDiscoveryStore } from '@/stores/mcpDiscovery'

const emit = defineEmits<{ open: [serverId: string] }>()

const { t } = useI18n()
const store = useMcpDiscoveryStore()

const view = computed(() => store.viewOf(null))
const loading = computed(() => store.loading[''] ?? false)
const error = computed(() => store.errors[''] ?? null)

const meta = computed(() => {
  const v = view.value
  if (!v) return ''
  const parts = [t('mcp.discovery.blockedCount', { count: v.blockedCount })]
  const at = v.scannedAt ? Date.parse(v.scannedAt) : NaN
  if (!Number.isNaN(at)) parts.push(t('mcp.discovery.readAt', { time: clockText(at) }))
  if (v.probing) parts.push(t('mcp.discovery.probing'))
  return parts.join(' · ')
})

onMounted(() => void store.watch(null))
onUnmounted(() => store.unwatch(null))
</script>

<template>
  <section class="flex min-w-0 flex-1 flex-col">
    <DetailHeader :title="t('mcp.discovery.globalTitle')">
      <template #meta>
        <span :title="t('mcp.discovery.blockedTitle')">{{ meta }}</span>
      </template>
      <template #actions>
        <Button
          variant="outline"
          size="sm"
          :disabled="loading"
          :title="t('mcp.discovery.reloadTitle')"
          @click="store.reload(null)"
        >
          <RotateCw class="h-3.5 w-3.5" :class="{ 'animate-spin': loading || view?.probing }" :stroke-width="1.75" />
          {{ t('mcp.discovery.reload') }}
        </Button>
      </template>
    </DetailHeader>

    <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      <div class="max-w-2xl space-y-3">
        <p class="text-xs leading-relaxed text-muted-foreground">{{ t('mcp.discovery.globalHint') }}</p>
        <Banner v-if="error" tone="destructive">{{ t('mcp.discovery.loadFailed', { error }) }}</Banner>
        <div v-if="!view && loading" class="space-y-1.5">
          <div v-for="i in 3" :key="i" class="h-10 animate-pulse rounded-md bg-muted/60" />
        </div>
        <div v-else-if="view" class="rounded-md border border-border">
          <McpDiscoveryRows
            :rows="view.global"
            :profiles="view.profiles"
            :problems="view.problems"
            :probing="view.probing"
            @open="emit('open', $event)"
          />
        </div>
      </div>
    </div>
  </section>
</template>
