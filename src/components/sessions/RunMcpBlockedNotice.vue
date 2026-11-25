<script setup lang="ts">
// MCP servers Puppeteer does not manage, blocked in this session: the engine
// asked for a tool of one, or still started one, and the sidecar denied it
// (backend `run:mcp_blocked`). Shown whatever the run status, until dismissed,
// so the user learns why a tool they expected did not work. Server names are
// another tool's config text: rendered as text only.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { X } from 'lucide-vue-next'
import { Banner } from '@/components/ui'
import { useRunMcpBlockedStore } from '@/stores/runMcpBlocked'

const props = defineProps<{ runId: string | null }>()
const { t } = useI18n()
const store = useRunMcpBlockedStore()
const blocked = computed(() => store.blockedOf(props.runId))
</script>

<template>
  <Banner v-if="runId && blocked.length" tone="warning" class="mb-2" data-run-mcp-blocked>
    <p>{{ t('run.mcpBlockedTitle') }}</p>
    <ul class="mt-0.5 space-y-0.5">
      <li v-for="b in blocked" :key="`${b.source}:${b.server}`">
        <span class="font-mono">{{ b.server }}</span>
        <span class="text-muted-foreground">
          · {{ t('run.mcpBlockedSource', { source: b.source }) }}<template v-if="b.calls">
            · {{ t('run.mcpBlockedCalls', { count: b.calls }) }}</template>
        </span>
      </li>
    </ul>
    <p class="mt-0.5 text-muted-foreground">{{ t('run.mcpBlockedHint') }}</p>
    <template #actions>
      <button
        type="button"
        class="h-5 w-5 shrink-0 rounded flex items-center justify-center hover:bg-foreground/10 cursor-pointer"
        :title="t('run.mcpBlockedDismiss')"
        :aria-label="t('run.mcpBlockedDismiss')"
        @click="runId && store.dismiss(runId)"
      >
        <X class="h-3 w-3" :stroke-width="2.5" />
      </button>
    </template>
  </Banner>
</template>
