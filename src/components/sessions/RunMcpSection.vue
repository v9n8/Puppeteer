<script setup lang="ts">
// The MCP part of the run options popover (UX-27): the session's MCP servers,
// switched on or off for this session only. The list starts as the project's
// set; only what the user changes here is stored, so a server they leave alone
// keeps following the project. The engines load MCP when their process starts,
// so a change reaches the session from its next start or resume.
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Server, RotateCcw } from 'lucide-vue-next'
import { Badge, ToggleSwitch } from '@/components/ui'
import { useMcpServersStore, type RunMcpEntry } from '@/stores/mcpServers'

const props = defineProps<{ runId: string; running: boolean }>()

const { t } = useI18n()
const mcpStore = useMcpServersStore()

const entries = ref<RunMcpEntry[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
/** The key being saved, or `*` while resetting them all. */
const saving = ref<string | null>(null)

const overridden = computed(() => entries.value.filter((e) => e.source === 'override'))

async function load() {
  const runId = props.runId
  loading.value = true
  error.value = null
  try {
    const list = await mcpStore.listForRun(runId)
    if (runId === props.runId) entries.value = list
  } catch (e) {
    error.value = t('run.mcpLoadFailed', { error: String(e) })
  } finally {
    loading.value = false
  }
}

async function toggle(entry: RunMcpEntry) {
  if (saving.value) return
  saving.value = entry.key
  error.value = null
  try {
    entries.value = await mcpStore.setRunOverride(props.runId, entry.key, !entry.enabled)
  } catch (e) {
    error.value = t('run.mcpSaveFailed', { error: String(e) })
  } finally {
    saving.value = null
  }
}

async function resetToProject() {
  if (saving.value) return
  saving.value = '*'
  error.value = null
  try {
    for (const entry of overridden.value) {
      entries.value = await mcpStore.setRunOverride(props.runId, entry.key, null)
    }
  } catch (e) {
    error.value = t('run.mcpSaveFailed', { error: String(e) })
  } finally {
    saving.value = null
  }
}

onMounted(load)
watch(() => props.runId, load)
</script>

<template>
  <div class="space-y-1.5">
    <div class="flex items-center gap-1.5 text-2xs font-medium uppercase text-muted-foreground">
      <Server class="h-3 w-3 shrink-0" :stroke-width="1.5" />
      <span>{{ t('run.mcpSetting') }}</span>
      <button
        v-if="overridden.length"
        type="button"
        class="ml-auto inline-flex items-center gap-1 normal-case text-primary hover:underline disabled:opacity-50"
        :disabled="saving !== null"
        :title="t('run.mcpResetTitle')"
        @click="resetToProject"
      >
        <RotateCcw class="h-3 w-3" :stroke-width="1.75" />
        {{ t('run.mcpReset') }}
      </button>
    </div>
    <div v-if="loading && !entries.length" class="space-y-1.5 py-1">
      <div v-for="i in 2" :key="i" class="h-3 w-full rounded bg-muted animate-pulse" />
    </div>
    <p v-else-if="!entries.length && !error" class="text-2xs text-muted-foreground">{{ t('run.mcpEmpty') }}</p>
    <ul v-else class="max-h-44 overflow-y-auto -mx-1">
      <li
        v-for="entry in entries"
        :key="entry.key"
        class="flex items-center gap-2 rounded px-1 py-1"
      >
        <span class="min-w-0 flex-1 flex items-center gap-1 flex-wrap">
          <span
            class="truncate font-mono text-xs"
            :class="entry.available ? 'text-foreground' : 'text-muted-foreground line-through'"
            :title="entry.description || entry.name"
          >{{ entry.name }}</span>
          <Badge
            v-if="entry.source === 'override'"
            tone="info"
            size="xs"
            class="shrink-0"
            :title="entry.enabled_for_project ? t('run.mcpOverrideOffTitle') : t('run.mcpOverrideOnTitle')"
          >{{ t('run.mcpOverride') }}</Badge>
          <Badge
            v-if="!entry.available"
            tone="neutral"
            size="xs"
            class="shrink-0"
            :title="t('run.mcpUnavailableTitle')"
          >{{ t('run.mcpUnavailable') }}</Badge>
        </span>
        <ToggleSwitch
          :checked="entry.enabled"
          :disabled="saving !== null"
          :loading="saving === entry.key"
          :label="entry.enabled ? t('run.mcpTurnOff', { name: entry.name }) : t('run.mcpTurnOn', { name: entry.name })"
          @toggle="toggle(entry)"
        />
      </li>
    </ul>
    <p v-if="error" class="text-2xs text-destructive">{{ error }}</p>
    <p class="text-2xs text-muted-foreground leading-snug">
      {{ running ? t('run.mcpAppliesAfterRun') : t('run.mcpAppliesNextMessage') }}
    </p>
  </div>
</template>
