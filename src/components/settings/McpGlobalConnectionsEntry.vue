<script setup lang="ts">
// "Global connections" entry pinned on top of the MCP sidebar: opens the list of
// MCP servers found outside Puppeteer in the detail pane, and shows how many of
// them runs block (FR-10) without opening it.
import { computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Globe } from 'lucide-vue-next'
import { rowClass } from '@/components/listDetail/rowStyles'
import { useMcpDiscoveryStore } from '@/stores/mcpDiscovery'

defineProps<{ selected: boolean }>()
const emit = defineEmits<{ select: [] }>()

const { t } = useI18n()
const store = useMcpDiscoveryStore()

const view = computed(() => store.viewOf(null))
const blocked = computed(() => view.value?.blockedCount ?? 0)

onMounted(() => void store.watch(null))
onUnmounted(() => store.unwatch(null))
</script>

<template>
  <button type="button" class="cursor-pointer text-left" :class="rowClass(selected)" @click="emit('select')">
    <Globe class="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
    <span class="flex min-w-0 flex-1 flex-col">
      <span class="truncate text-sm font-medium leading-snug">{{ t('mcp.discovery.globalTitle') }}</span>
      <span class="mt-1 truncate text-2xs text-muted-foreground/70">
        {{ view?.probing ? t('mcp.discovery.probing') : t('mcp.discovery.blockedCount', { count: blocked }) }}
      </span>
    </span>
    <span
      v-if="blocked > 0"
      class="mt-0.5 shrink-0 rounded-full bg-warning/15 px-1.5 text-2xs font-semibold tabular-nums text-warning"
      :title="t('mcp.discovery.blockedTitle')"
    >{{ blocked }}</span>
  </button>
</template>
