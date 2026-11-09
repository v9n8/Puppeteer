<script setup lang="ts">
// Import from other AI tools (`library.import_external`): pick a tool and where to
// read it (Home or a project), see what it holds, tick items, rename them or pick
// their engine, then copy them into the libraries. The tools are whatever adapters
// the backend lists, so a new one shows up here without a change.
//
// The commands only read this machine's tool configs (`~/.claude.json` holds the
// account), so they are local-only: a remote desktop window gets a notice instead.
// After an import the libraries it filled are listed again, so a skill shows on the
// Skills Allocation matrix right away.
import { computed, onMounted, ref, shallowRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { RotateCw } from 'lucide-vue-next'
import { AppSelect, Badge, Banner, Button, Input, PageHeader } from '@/components/ui'
import { invoke } from '@/lib/tauri'
import { errorText } from '@/lib/featureGate'
import { isRemoteMode } from '@/lib/ipc/remote'
import { useFeaturesStore } from '@/stores/features'
import { useSkillsStore } from '@/stores/skills'
import { useRulesStore } from '@/stores/rules'
import { useMcpServersStore } from '@/stores/mcpServers'
import { useLibraryItemsStore } from '@/stores/libraryItems'
import {
  CMD_IMPORT_CANDIDATES,
  CMD_LIST_IMPORT_SOURCES,
  CMD_SCAN_IMPORT_SOURCE,
  TARGETS,
  buildPicks,
  candidateSections,
  conflictOf,
  countOutcomes,
  hasTarget,
  importReady,
  importedKinds,
  initialChoices,
  nameError,
  sourceAdapters,
  sourceKey,
  sourceRef,
  visibleSources,
  withAllPicked,
  withChoice,
  type ImportCandidate,
  type ImportChoice,
  type ImportChoices,
  type ImportKind,
  type ImportOutcome,
  type ImportScan,
  type ImportSource,
  type ImportTarget,
  type OnConflict,
} from '@/lib/configImport'

defineOptions({ name: 'ConfigImportView' })

const { t, te } = useI18n()
const features = useFeaturesStore()
const remote = isRemoteMode()

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

const shown = computed(() => visibleSources(sources.value, features.isOn))
const adapterOptions = computed(() => sourceAdapters(shown.value).map((id) => ({ value: id, label: adapterLabel(id) })))
const sourceOptions = computed(() =>
  shown.value
    .filter((s) => s.adapter === adapter.value)
    .map((s) => ({
      value: sourceKey(sourceRef(s)),
      label: s.scope === 'global' ? t('configImport.scopeHome') : (s.project_name ?? s.root),
      description: s.kinds.map((k) => t(`configImport.kind.${k}`)).join(' · '),
    })),
)
const selected = computed(() => shown.value.find((s) => sourceKey(sourceRef(s)) === selectedKey.value) ?? null)

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

const targetOptions = computed(() => TARGETS.map((v) => ({ value: v, label: t(`configImport.target.${v}`) })))
const conflictOptions = computed(() => [
  { value: 'rename', label: t('configImport.conflict.rename') },
  { value: 'skip', label: t('configImport.conflict.skip') },
])

function adapterLabel(id: string): string {
  const key = `configImport.adapter.${id}`
  return te(key) ? t(key) : id
}

async function loadSources() {
  if (remote) return
  loadingSources.value = true
  error.value = null
  try {
    sources.value = await invoke<ImportSource[]>(CMD_LIST_IMPORT_SOURCES)
  } catch (e) {
    sources.value = []
    error.value = errorText(e, t)
  } finally {
    loadingSources.value = false
  }
}

// Keep the tool and source while they are still listed, else the first one.
watch(adapterOptions, (opts) => {
  if (!opts.some((o) => o.value === adapter.value)) adapter.value = opts[0]?.value ?? ''
})
watch(sourceOptions, (opts) => {
  if (!opts.some((o) => o.value === selectedKey.value)) selectedKey.value = opts[0]?.value ?? ''
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
    const report = await invoke<ImportScan>(CMD_SCAN_IMPORT_SOURCE, { source: sourceRef(source) })
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

onMounted(loadSources)

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

// The libraries an import filled, listed again for the screens that show them.
async function refreshLibraries(kinds: ImportKind[]) {
  const fetchers: Record<ImportKind, () => Promise<unknown>> = {
    mcp: () => useMcpServersStore().fetchServers(),
    rule: () => useRulesStore().fetchRules(),
    skill: () => useSkillsStore().fetchSkills(),
    agent: () => useLibraryItemsStore('agent').fetchItems(),
    command: () => useLibraryItemsStore('command').fetchItems(),
  }
  await Promise.allSettled(kinds.map((k) => fetchers[k]()))
}

async function runImport() {
  const source = selected.value
  if (!source || !ready.value || importing.value) return
  importing.value = true
  error.value = null
  const picks = buildPicks(candidates.value, choices.value)
  const wanted = new Map(picks.map((p) => [p.id, choices.value[p.id].name]))
  try {
    const outcomes = await invoke<ImportOutcome[]>(CMD_IMPORT_CANDIDATES, { source: sourceRef(source), picks })
    lastImport.value = { outcomes, wanted }
    await refreshLibraries(importedKinds(outcomes))
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
  <div class="flex flex-col h-full">
    <PageHeader :title="t('configImport.title')">
      <template v-if="!remote" #actions>
        <Button variant="ghost" size="xs" :disabled="loadingSources || scanning" @click="refresh">
          <RotateCw class="h-3.5 w-3.5" :class="(loadingSources || scanning) && 'animate-spin'" :stroke-width="2" />
          {{ t('configImport.refresh') }}
        </Button>
      </template>
    </PageHeader>

    <div class="flex-1 overflow-auto p-6">
      <div class="max-w-4xl space-y-4">
        <Banner v-if="remote" tone="info">{{ t('configImport.remote') }}</Banner>

        <template v-else>
          <p class="text-xs text-muted-foreground">{{ t('configImport.hint') }}</p>

          <Banner v-if="error" tone="destructive">{{ error }}</Banner>

          <p v-if="!loadingSources && !error && shown.length === 0" class="text-xs text-muted-foreground">
            {{ t('configImport.noSources') }}
          </p>

          <div v-if="shown.length > 0" class="flex flex-wrap items-center gap-2">
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
                    <AppSelect
                      v-if="hasTarget(c.kind)"
                      :model-value="choices[c.id]?.target ?? 'claude'"
                      :options="targetOptions"
                      size="xs"
                      class="w-28"
                      :disabled="!choices[c.id]?.picked"
                      @update:model-value="setChoice(c.id, { target: $event as ImportTarget })"
                    />
                    <template v-if="conflictOf(candidates, choices, c.id)">
                      <Badge tone="warning" size="xs">{{ t(`configImport.conflict.${conflictOf(candidates, choices, c.id)}`) }}</Badge>
                      <AppSelect
                        :model-value="choices[c.id]?.onConflict ?? 'rename'"
                        :options="conflictOptions"
                        size="xs"
                        class="w-44"
                        @update:model-value="setChoice(c.id, { onConflict: $event as OnConflict })"
                      />
                    </template>
                    <Badge v-else-if="c.exists && !choices[c.id]?.picked" size="xs">{{ t('configImport.conflict.exists') }}</Badge>
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

          <div v-if="candidates.length > 0" class="flex items-center gap-3">
            <Button :disabled="!ready || importing || scanning" @click="runImport">
              {{ importing ? t('configImport.importing') : t('configImport.import', { count: pickedCount }) }}
            </Button>
            <span class="text-2xs text-muted-foreground">{{ t('configImport.copyOnly') }}</span>
          </div>

          <Banner v-if="counts" :tone="counts.failed > 0 ? 'warning' : 'success'">
            <p>{{ t('configImport.outcome.summary', counts) }}</p>
            <ul v-if="notable.length > 0" class="mt-1 space-y-0.5">
              <li v-for="o in notable" :key="o.id" class="break-all">{{ outcomeLabel(o) }}: {{ outcomeText(o) }}</li>
            </ul>
          </Banner>
        </template>
      </div>
    </div>
  </div>
</template>
