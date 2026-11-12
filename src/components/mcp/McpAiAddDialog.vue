<script setup lang="ts">
// "Add with AI" (UX-87): paste setup text or a README link; the backend reads a known format or
// asks the AI (`draft_mcp_servers`) on the engine / model / account picked here, which start on
// the defaults of Settings. One server opens as a draft right away, several are listed to pick
// from. Nothing is saved here — the user finishes and saves the draft in the pane.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Loader2, Sparkles } from 'lucide-vue-next'
import { AppSelect, Banner, Button, InfoTip, Modal, Textarea } from '@/components/ui'
import { useMcpServersStore, type McpSetupTarget, type PastedServer } from '@/stores/mcpServers'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useClaudeAccountsStore } from '@/stores/claudeAccounts'
import { useCodexAccountsStore } from '@/stores/codexAccounts'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { MODEL_OPTIONS, withCurrentModel, type SelectOption } from '@/lib/engineOptions'
import { composerDefaults } from '@/lib/composerChips'
import { setupTargetFor, type SetupEngine } from '@/lib/mcpRows'

const props = defineProps<{ open: boolean }>()

const emit = defineEmits<{
  'update:open': [open: boolean]
  /** `byAi`: read by the AI, not a known format. */
  pick: [server: PastedServer, byAi: boolean]
}>()

const { t } = useI18n()
const store = useMcpServersStore()
const appSettings = useAppSettingsStore()
const claudeStore = useClaudeAccountsStore()
const codexStore = useCodexAccountsStore()
const modelCatalog = useModelCatalogStore()

// The text survives closing without a pick, so a retry needs no re-paste.
const text = ref('')
const reading = ref(false)
const error = ref<string | null>(null)
const found = ref<PastedServer[]>([])
const byAi = ref(false)
const target = ref<McpSetupTarget>({ engine: 'claude', model: '', accountId: '' })

function pickEngine(engine: SetupEngine) {
  target.value = setupTargetFor(engine, composerDefaults({ settings: appSettings.settings }).models, {
    claude: claudeStore.accounts,
    codex: codexStore.accounts,
  })
}

// Every open starts on the defaults again: a pick here is for this request only.
watch(
  () => props.open,
  async (open) => {
    if (!open) return
    error.value = null
    found.value = []
    await Promise.all([
      appSettings.ensureLoaded(),
      claudeStore.accounts.length ? null : claudeStore.fetch(),
      codexStore.accounts.length ? null : codexStore.fetch(),
      modelCatalog.ensureLoaded(),
    ]).catch((e) => {
      error.value = String(e)
    })
    pickEngine(composerDefaults({ settings: appSettings.settings }).engine === 'codex' ? 'codex' : 'claude')
  },
  { immediate: true },
)

const engineOptions: SelectOption[] = [
  { value: 'claude', label: 'Claude' },
  { value: 'codex', label: 'Codex' },
]
const modelOptions = computed<SelectOption[]>(() => {
  const { engine, model } = target.value
  const base = engine === 'codex'
    ? modelCatalog.codexOptions(MODEL_OPTIONS.codex)
    : modelCatalog.withValidation(MODEL_OPTIONS.claude)
  return withCurrentModel(base, model).map((o) => (o.value ? o : { ...o, label: t('settings.ai.modelDefault') }))
})
// Without a default account the engine's own login runs; it is offered as the empty choice.
const accountOptions = computed<SelectOption[]>(() => {
  const codex = target.value.engine === 'codex'
  const accounts = codex ? codexStore.accounts : claudeStore.accounts
  const own = accounts.some((a) => a.is_default)
    ? []
    : [{ value: '', label: codex ? t('settings.accounts.sharedCodex') : t('projectDetail.sessionDefaults.noAccount') }]
  return [...own, ...accounts.map((a) => ({ value: a.id, label: a.label }))]
})

async function read() {
  if (!text.value.trim() || reading.value) return
  reading.value = true
  error.value = null
  found.value = []
  try {
    const drafts = await store.draftServers(text.value, target.value)
    byAi.value = drafts.by_ai
    if (drafts.servers.length === 0) error.value = t('mcp.aiAdd.none')
    else if (drafts.servers.length === 1) pick(drafts.servers[0])
    else found.value = drafts.servers
  } catch (e) {
    error.value = String(e)
  } finally {
    reading.value = false
  }
}

function pick(server: PastedServer) {
  emit('pick', server, byAi.value)
  emit('update:open', false)
  text.value = ''
  found.value = []
}

function onKeydown(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
    e.preventDefault()
    void read()
  }
}
</script>

<template>
  <Modal
    :open="open"
    size="lg"
    :title="t('mcp.aiAdd.title')"
    :dismiss-on-overlay="false"
    @update:open="emit('update:open', $event)"
  >
    <div class="space-y-3 p-4">
      <p class="text-xs text-muted-foreground">{{ t('mcp.aiAdd.hint') }}</p>
      <Textarea
        v-model="text"
        rows="8"
        class="font-mono text-xs"
        :placeholder="t('mcp.aiAdd.placeholder')"
        :disabled="reading"
        @keydown="onKeydown"
      />
      <div class="space-y-1.5">
        <div class="flex items-center gap-1.5">
          <span class="text-xs font-medium">{{ t('mcp.aiAdd.runOn') }}</span>
          <InfoTip :text="t('mcp.aiAdd.runOnHint')" />
        </div>
        <div class="grid grid-cols-3 gap-2">
          <label class="min-w-0 space-y-1">
            <span class="text-2xs text-muted-foreground">{{ t('settings.ai.defaultEngine') }}</span>
            <AppSelect
              size="sm"
              :model-value="target.engine"
              :options="engineOptions"
              :disabled="reading"
              @update:model-value="pickEngine($event === 'codex' ? 'codex' : 'claude')"
            />
          </label>
          <label class="min-w-0 space-y-1">
            <span class="text-2xs text-muted-foreground">{{ t('settings.engines.models') }}</span>
            <AppSelect v-model="target.model" size="sm" :options="modelOptions" :disabled="reading" />
          </label>
          <label class="min-w-0 space-y-1">
            <span class="text-2xs text-muted-foreground">{{ t('settings.engines.accounts') }}</span>
            <AppSelect v-model="target.accountId" size="sm" :options="accountOptions" :disabled="reading" />
          </label>
        </div>
      </div>
      <p class="text-2xs text-muted-foreground">{{ t('mcp.aiAdd.privacy') }}</p>

      <Banner v-if="error" tone="warning">{{ error }}</Banner>

      <div v-if="found.length > 0" class="space-y-1.5">
        <p class="text-xs">{{ t('mcp.aiAdd.pick', { count: found.length }) }}</p>
        <button
          v-for="s in found"
          :key="s.name"
          type="button"
          class="flex w-full items-center gap-2 rounded-md border border-border px-3 py-2 text-left transition-colors hover:bg-accent/50"
          @click="pick(s)"
        >
          <span class="font-mono text-sm font-medium">{{ s.name }}</span>
          <span class="shrink-0 rounded bg-muted px-1 text-2xs font-semibold uppercase tracking-wide text-muted-foreground">
            {{ s.transport }}
          </span>
          <span class="min-w-0 flex-1 truncate font-mono text-2xs text-muted-foreground">
            {{ s.url ?? [s.command, ...s.args].join(' ') }}
          </span>
        </button>
      </div>
    </div>

    <template #footer>
      <Button variant="ghost" @click="emit('update:open', false)">{{ t('common.cancel') }}</Button>
      <Button :disabled="!text.trim() || reading" @click="read">
        <Loader2 v-if="reading" class="h-3.5 w-3.5 animate-spin" :stroke-width="2" />
        <Sparkles v-else class="h-3.5 w-3.5" :stroke-width="1.75" />
        {{ reading ? t('mcp.aiAdd.reading') : t('mcp.aiAdd.read') }}
      </Button>
    </template>
  </Modal>
</template>
