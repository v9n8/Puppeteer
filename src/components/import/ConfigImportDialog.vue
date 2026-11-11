<script setup lang="ts">
// Import from other AI tools (`library.import_external`, UX-56): pick a tool and where
// to read it (Home or a project), see what it holds, tick items, rename them, then
// copy them to `dest` — the Library's scope (rules and skills become files in its
// `.claude/` folders) or the MCP list. The tools are whatever adapters the backend
// lists, so a new one shows up here without a change.
//
// The commands only read this machine's tool configs (`~/.claude.json` holds the
// account) and write files, so they are local-only: the screens that open this
// dialog hide their Import button on a remote desktop.
import { computed, ref, shallowRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { RotateCw } from 'lucide-vue-next'
import { AppSelect, Badge, Banner, Button, Input, Modal } from '@/components/ui'
import { invoke } from '@/lib/tauri'
import { errorText } from '@/lib/featureGate'
import {
  CMD_IMPORT_CANDIDATES,
  CMD_LIST_IMPORT_SOURCES,
  CMD_SCAN_IMPORT_SOURCE,
  CONFLICT_CHOICES,
  buildPicks,
  candidateSections,
  conflictOf,
  countOutcomes,
  importReady,
  initialChoices,
  nameError,
  readsDest,
  sourceAdapters,
  sourceKey,
  sourceRef,
  withAllPicked,
  withChoice,
  type ImportCandidate,
  type ImportChoice,
  type ImportChoices,
  type ImportDest,
  type ImportOutcome,
  type ImportScan,
  type ImportSource,
  type OnConflict,
} from '@/lib/configImport'

const props = defineProps<{
  open: boolean
  dest: ImportDest
  /** Where a Library import writes, shown as is (`~/.claude/`, `<project>/.claude/`). */
  destLabel?: string
}>()
const emit = defineEmits<{ 'update:open': [value: boolean]; imported: [] }>()

const { t, te } = useI18n()
const into = computed(() => props.dest.into)

const sources = shallowRef<ImportSource[]>([])
const loadingSources = ref(false)
const adapter = ref('')
const selectedKey = ref('')

const scan = shallowRef<ImportScan | null>(null)
const scanning = ref(false)
const choices = ref<ImportChoices>({})
const importing = ref(false)
/** The last import's outcomes, with the name each pick asked for. */
const lastImport = shallowRef<{ outcomes: ImportOutcome[]; wanted: Map<string, string> } | null>(null)
const error = ref<string | null>(null)

const adapterOptions = computed(() => sourceAdapters(sources.value).map((id) => ({ value: id, label: adapterLabel(id) })))
const sourceOptions = computed(() =>
  sources.value
    .filter((s) => s.adapter === adapter.value)
    .map((s) => ({
      value: sourceKey(sourceRef(s)),
      label: s.scope === 'global' ? t('configImport.scopeHome') : (s.project_name ?? s.root),
      description: s.kinds.map((k) => t(`configImport.kind.${k}`)).join(' · '),
    })),
)
const selected = computed(() => sources.value.find((s) => sourceKey(sourceRef(s)) === selectedKey.value) ?? null)

const candidates = computed(() => scan.value?.candidates ?? [])
const sections = computed(() => candidateSections(candidates.value))
const pickedCount = computed(() => candidates.value.filter((c) => choices.value[c.id]?.picked).length)
const ready = computed(() => importReady(candidates.value, choices.value))
const counts = computed(() => (lastImport.value ? countOutcomes(lastImport.value.outcomes) : null))
/** Outcomes worth a line: not imported, or imported under another name than asked. */
const notable = computed(() => {
  const last = lastImport.value
  return last ? last.outcomes.filter((o) => o.status !== 'imported' || o.name !== last.wanted.get(o.id)) : []
})

const conflictOptions = computed(() =>
  CONFLICT_CHOICES[into.value].map((v) => ({ value: v, label: t(`configImport.conflict.${v}`) })),
)

function adapterLabel(id: string): string {
  const key = `configImport.adapter.${id}`
  return te(key) ? t(key) : id
}

function conflictLabel(id: string): string {
  const kind = conflictOf(candidates.value, choices.value, id)
  if (kind === 'exists') return t(into.value === 'mcp' ? 'configImport.conflict.existsMcp' : 'configImport.conflict.existsFile')
  return kind ? t('configImport.conflict.duplicate') : ''
}

async function loadSources() {
  loadingSources.value = true
  error.value = null
  try {
    sources.value = await invoke<ImportSource[]>(CMD_LIST_IMPORT_SOURCES, { into: into.value })
  } catch (e) {
    sources.value = []
    error.value = errorText(e, t)
  } finally {
    loadingSources.value = false
  }
}

// Keep the tool and source while they are still listed; else the one reading the
// place the import writes to (a project's own Cursor rules), else the first.
watch(adapterOptions, (opts) => {
  if (opts.some((o) => o.value === adapter.value)) return
  adapter.value = sources.value.find((s) => readsDest(s, props.dest))?.adapter ?? opts[0]?.value ?? ''
})
watch(sourceOptions, (opts) => {
  if (opts.some((o) => o.value === selectedKey.value)) return
  const own = sources.value.find((s) => s.adapter === adapter.value && readsDest(s, props.dest))
  selectedKey.value = own ? sourceKey(sourceRef(own)) : (opts[0]?.value ?? '')
})

// Bumped by every scan, so a slower stale answer is dropped.
let seq = 0

async function runScan() {
  const source = selected.value
  const mine = ++seq
  if (!source) {
    scan.value = null
    choices.value = {}
    return
  }
  scanning.value = true
  try {
    const report = await invoke<ImportScan>(CMD_SCAN_IMPORT_SOURCE, { source: sourceRef(source), dest: props.dest })
    if (mine !== seq) return
    scan.value = report
    choices.value = initialChoices(report.candidates)
    error.value = null
  } catch (e) {
    if (mine !== seq) return
    scan.value = null
    choices.value = {}
    error.value = errorText(e, t)
  } finally {
    if (mine === seq) scanning.value = false
  }
}

watch(selectedKey, () => {
  lastImport.value = null
  void runScan()
})

// Each opening starts over: the destination may have changed, and so may the disk.
watch(
  () => props.open,
  (open) => {
    if (!open) return
    adapter.value = ''
    selectedKey.value = ''
    scan.value = null
    choices.value = {}
    lastImport.value = null
    void loadSources()
  },
  { immediate: true },
)

async function refresh() {
  lastImport.value = null
  await loadSources()
  await runScan()
}

function setChoice(id: string, patch: Partial<ImportChoice>) {
  choices.value = withChoice(choices.value, id, patch)
}

function setSection(list: ImportCandidate[], picked: boolean) {
  choices.value = withAllPicked(choices.value, list.map((c) => c.id), picked)
}

function sectionAllPicked(list: ImportCandidate[]): boolean {
  return list.every((c) => choices.value[c.id]?.picked)
}

function nameHint(id: string): string | null {
  const err = nameError(choices.value[id]?.name ?? '')
  return err ? t(`configImport.nameError.${err}`) : null
}

function mcpLine(c: ImportCandidate): string {
  const m = c.mcp
  if (!m) return ''
  const where = m.command ? [m.command, ...m.args].join(' ') : (m.url ?? '')
  const keys = [...m.env_keys, ...m.header_keys]
  return [m.transport, where, keys.length ? t('configImport.secretKeys', { keys: keys.join(', ') }) : '']
    .filter(Boolean)
    .join(' · ')
}

async function runImport() {
  const source = selected.value
  if (!source || !ready.value || importing.value) return
  importing.value = true
  error.value = null
  const picks = buildPicks(candidates.value, choices.value)
  const wanted = new Map(picks.map((p) => [p.id, choices.value[p.id].name]))
  try {
    const outcomes = await invoke<ImportOutcome[]>(CMD_IMPORT_CANDIDATES, {
      source: sourceRef(source),
      dest: props.dest,
      picks,
    })
    lastImport.value = { outcomes, wanted }
    if (outcomes.some((o) => o.status === 'imported')) emit('imported')
    // Imported names now exist: scan again so they show as taken.
    await runScan()
  } catch (e) {
    error.value = errorText(e, t)
  } finally {
    importing.value = false
  }
}

function outcomeLabel(o: ImportOutcome): string {
  const cand = candidates.value.find((c) => c.id === o.id)
  return cand ? `${t(`configImport.kind.${cand.kind}`)} · ${cand.name}` : o.id
}

function outcomeText(o: ImportOutcome): string {
  if (o.status === 'imported') return t('configImport.outcome.renamed', { name: o.name ?? '' })
  if (o.status === 'skipped') return t('configImport.outcome.skipped', { name: o.name ?? '' })
  return o.message ?? t('configImport.outcome.failed')
}
</script>

<template>
  <Modal
    :open="open"
    size="2xl"
    scroll-body
    :dismiss-on-overlay="false"
    @update:open="emit('update:open', $event)"
  >
    <template #header>
      <h3 class="flex-1 text-sm font-semibold">{{ t('configImport.title') }}</h3>
      <Button
        variant="ghost"
        size="icon"
        :disabled="loadingSources || scanning"
        :title="t('configImport.refresh')"
        :aria-label="t('configImport.refresh')"
        @click="refresh"
      >
        <RotateCw class="h-3.5 w-3.5" :class="(loadingSources || scanning) && 'animate-spin'" :stroke-width="1.75" />
      </Button>
    </template>

    <div class="space-y-4 p-4">
      <p class="text-xs text-muted-foreground">
        {{ into === 'mcp' ? t('configImport.hintMcp') : t('configImport.hintLibrary') }}
      </p>
      <p v-if="into === 'library' && destLabel" class="truncate text-xs" :title="destLabel">
        <span class="text-muted-foreground">{{ t('configImport.into') }}</span>
        <span class="ml-1 font-mono">{{ destLabel }}</span>
      </p>

      <Banner v-if="error" tone="destructive">{{ error }}</Banner>

      <p v-if="!loadingSources && !error && sources.length === 0" class="text-xs text-muted-foreground">
        {{ into === 'mcp' ? t('configImport.noSourcesMcp') : t('configImport.noSourcesLibrary') }}
      </p>

      <div v-if="sources.length > 0" class="flex flex-wrap items-center gap-2">
        <label class="text-xs text-muted-foreground">{{ t('configImport.tool') }}</label>
        <AppSelect v-model="adapter" :options="adapterOptions" class="w-44" />
        <label class="ml-2 text-xs text-muted-foreground">{{ t('configImport.from') }}</label>
        <AppSelect v-model="selectedKey" :options="sourceOptions" class="w-64" />
        <span v-if="selected" class="min-w-0 truncate text-2xs text-muted-foreground" :title="selected.root">
          {{ selected.root }}
        </span>
      </div>

      <Banner v-if="scan && scan.problems.length > 0" tone="warning">
        <p>{{ t('configImport.problems', { count: scan.problems.length }) }}</p>
        <ul class="mt-1 space-y-0.5">
          <li v-for="(p, i) in scan.problems" :key="i" class="break-all">
            <span class="font-mono">{{ p.path }}</span><template v-if="p.item"> ({{ p.item }})</template>: {{ p.message }}
          </li>
        </ul>
      </Banner>

      <p v-if="scanning && !scan" class="text-xs text-muted-foreground">{{ t('configImport.scanning') }}</p>
      <p v-else-if="scan && candidates.length === 0" class="text-xs text-muted-foreground">
        {{ t('configImport.noCandidates') }}
      </p>

      <div
        v-for="section in sections"
        :key="section.kind"
        class="rounded-lg border border-border bg-card"
        :class="scanning && 'opacity-60'"
      >
        <div class="flex items-center gap-2 border-b border-border/50 px-3 py-2 text-xs">
          <input
            type="checkbox"
            class="accent-primary"
            :checked="sectionAllPicked(section.candidates)"
            :aria-label="t('configImport.pickAll')"
            @change="setSection(section.candidates, !sectionAllPicked(section.candidates))"
          >
          <span class="font-semibold">{{ t(`configImport.kind.${section.kind}`) }}</span>
          <span class="text-2xs text-muted-foreground">({{ section.candidates.length }})</span>
        </div>
        <div class="divide-y divide-border/40">
          <div v-for="c in section.candidates" :key="c.id" class="flex items-start gap-2 px-3 py-2 text-xs">
            <input
              type="checkbox"
              class="mt-2 accent-primary"
              :checked="choices[c.id]?.picked"
              :aria-label="c.name"
              @change="setChoice(c.id, { picked: !choices[c.id]?.picked })"
            >
            <div class="min-w-0 flex-1 space-y-1">
              <div class="flex flex-wrap items-center gap-2">
                <Input
                  :model-value="choices[c.id]?.name ?? ''"
                  size="xs"
                  class="w-48"
                  :aria-label="t('configImport.name')"
                  :disabled="!choices[c.id]?.picked"
                  @update:model-value="setChoice(c.id, { name: String($event ?? '').trim() })"
                />
                <template v-if="conflictOf(candidates, choices, c.id)">
                  <Badge tone="warning" size="xs">{{ conflictLabel(c.id) }}</Badge>
                  <AppSelect
                    :model-value="choices[c.id]?.onConflict ?? 'rename'"
                    :options="conflictOptions"
                    size="xs"
                    class="w-52"
                    @update:model-value="setChoice(c.id, { onConflict: $event as OnConflict })"
                  />
                </template>
                <Badge v-else-if="c.exists && !choices[c.id]?.picked" size="xs">
                  {{ into === 'mcp' ? t('configImport.conflict.existsMcp') : t('configImport.conflict.existsFile') }}
                </Badge>
              </div>
              <p v-if="choices[c.id]?.picked && nameHint(c.id)" class="text-2xs text-destructive">{{ nameHint(c.id) }}</p>
              <p v-if="c.kind === 'mcp'" class="truncate text-2xs text-muted-foreground" :title="mcpLine(c)">{{ mcpLine(c) }}</p>
              <p v-else-if="c.description && c.description !== c.name" class="line-clamp-2 text-2xs text-muted-foreground">
                {{ c.description }}
              </p>
              <p class="truncate font-mono text-2xs text-muted-foreground/70" :title="c.origin">{{ c.origin }}</p>
            </div>
          </div>
        </div>
      </div>

      <Banner v-if="counts" :tone="counts.failed > 0 ? 'warning' : 'success'">
        <p>{{ t('configImport.outcome.summary', counts) }}</p>
        <ul v-if="notable.length > 0" class="mt-1 space-y-0.5">
          <li v-for="o in notable" :key="o.id" class="break-all">{{ outcomeLabel(o) }}: {{ outcomeText(o) }}</li>
        </ul>
      </Banner>
    </div>

    <template #footer>
      <span class="mr-auto text-2xs text-muted-foreground">{{ t('configImport.copyOnly') }}</span>
      <Button variant="ghost" @click="emit('update:open', false)">{{ t('common.close') }}</Button>
      <Button :disabled="!ready || importing || scanning" @click="runImport">
        {{ importing ? t('configImport.importing') : t('configImport.import', { count: pickedCount }) }}
      </Button>
    </template>
  </Modal>
</template>
