<script setup lang="ts">
// Right pane of the MCP servers screen for a managed claude.ai connector: it has
// no command / url, so nothing to edit, test, export or duplicate — only its
// app-wide switch (`set_mcp_connector_enabled`; the form's save refuses it) and
// Delete. Which Claude profile it belongs to and its run key come from the
// discovery row that points at it.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Trash2 } from 'lucide-vue-next'
import { Button, InfoTip, ToggleSwitch } from '@/components/ui'
import DetailHeader from '@/components/listDetail/DetailHeader.vue'
import DetailSection from '@/components/listDetail/DetailSection.vue'
import { useToast } from '@/composables/useToast'
import { errorText } from '@/lib/featureGate'
import { useMcpDiscoveryStore } from '@/stores/mcpDiscovery'
import type { McpServer } from '@/stores/mcpServers'

const props = defineProps<{ server: McpServer }>()
const emit = defineEmits<{ delete: [] }>()

const { t } = useI18n()
const { toast } = useToast()
const discovery = useMcpDiscoveryStore()

/** The discovery row of this copy (Settings' view, loaded by the sidebar entry). */
const row = computed(() => discovery.viewOf(null)?.global.find((r) => r.serverId === props.server.id) ?? null)
const toggling = computed(() => discovery.busy.has(props.server.id))

async function toggle() {
  const on = !props.server.enabled
  try {
    await discovery.setConnectorEnabled(props.server.id, on)
    toast.success(on ? t('mcp.list.toast.enabled') : t('mcp.list.toast.disabled'))
  } catch (e) {
    toast.error(errorText(e, t))
  }
}
</script>

<template>
  <section class="flex min-w-0 flex-1 flex-col">
    <DetailHeader :title="server.name">
      <template #meta>{{ t('mcp.connector.meta', { name: server.description }) }}</template>
      <template #actions>
        <Button variant="outline" size="sm" :title="t('common.delete')" @click="emit('delete')">
          <Trash2 class="h-3.5 w-3.5" :stroke-width="1.75" />
          {{ t('common.delete') }}
        </Button>
      </template>
    </DetailHeader>

    <div class="min-h-0 flex-1 overflow-y-auto px-5">
      <div class="max-w-xl">
        <DetailSection :title="t('mcp.detail.status')">
          <div class="flex items-center gap-3">
            <div class="flex min-w-0 flex-1 items-center gap-1.5">
              <span class="text-xs font-medium">{{ server.enabled ? t('mcp.editor.enabled') : t('mcp.editor.disabled') }}</span>
              <InfoTip :text="t('mcp.connector.enabledHint')" />
            </div>
            <ToggleSwitch
              :checked="server.enabled"
              :loading="toggling"
              :disabled="toggling"
              :label="server.enabled ? t('mcp.list.disable') : t('mcp.list.enable')"
              @toggle="toggle"
            />
          </div>
        </DetailSection>

        <DetailSection :title="t('mcp.detail.about')" :hint="t('mcp.connector.hint')" hint-visible>
          <dl class="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-xs leading-relaxed">
            <dt class="text-muted-foreground">{{ t('mcp.connector.claudeName') }}</dt>
            <dd>{{ server.description }}</dd>
            <template v-if="row">
              <dt class="text-muted-foreground">{{ t('mcp.connector.profile') }}</dt>
              <dd>{{ row.profile?.label ?? t('mcp.discovery.globalProfile') }}</dd>
              <dt class="text-muted-foreground">{{ t('mcp.connector.tools') }}</dt>
              <dd class="font-mono">mcp__{{ row.importName }}__*</dd>
              <template v-if="row.engineStatus">
                <dt class="text-muted-foreground">{{ t('mcp.connector.engineStatus') }}</dt>
                <dd>{{ row.engineStatus }}</dd>
              </template>
            </template>
          </dl>
          <p v-if="discovery.viewOf(null) && !row && !discovery.viewOf(null)?.probing" class="text-2xs text-warning">{{ t('mcp.connector.notFound') }}</p>
        </DetailSection>
      </div>
    </div>
  </section>
</template>
