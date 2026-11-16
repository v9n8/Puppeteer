<script setup lang="ts">
// Settings › Model (V3-63): which models each engine may use. One tab per engine,
// "Scan models" (Claude: `validate_claude_models`, a real turn per pinned id;
// Codex: `refresh_codex_models`), the "Available" / "Not available" blocks drawn
// from the SAVED list, and switches that only change a draft until "Save" (so a
// row does not jump while it is being changed). Saving goes through
// `save_enabled_models`: a dry run first, whose moved defaults the dialog lists
// (ModelSaveDialog), then the write; Settings, the shared settings and the
// projects are reloaded after it, since the save may move their defaults.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { emit as emitEvent } from '@tauri-apps/api/event'
import { AlertCircle, CircleDot, Folder, History, Loader2, Lock, RotateCw, ScanSearch } from 'lucide-vue-next'
import { Badge, Banner, Button, ToggleSwitch } from '@/components/ui'
import { invoke } from '@/lib/tauri'
import { errorText } from '@/lib/featureGate'
import { useToast } from '@/composables/useToast'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { useProjectsStore } from '@/stores/projects'
import { CLAUDE_MODEL_IDS, MODEL_OPTIONS, codexOptions } from '@/lib/engineOptions'
import { draftOn, draftSummary, engineListToSave, modelBlocks, parseEnabledModels, type ModelRow } from '@/lib/enabledModels'
import { modelDefaultsUsage, type DefaultChange, type ModelEngine } from '@/lib/modelDefaultsUsage'
import { projectPageQuery, sectionFromQuery } from '@/lib/projectSections'
import { SCREEN_OPEN_ROUTE, type ScreenOpenRoute } from '@/lib/detachedScreens'
import type { SettingsSectionId } from '@/lib/settingsSections'
import { useSettingsForm } from './useSettingsForm'
import ModelSaveDialog from './ModelSaveDialog.vue'

const props = defineProps<{
  /** The section is on screen: ⌘S saves, and opening it reloads the lists. */
  active: boolean
}>()
const emit = defineEmits<{ openSection: [id: SettingsSectionId] }>()

const { t, locale } = useI18n()
const { toast } = useToast()
const appSettings = useAppSettingsStore()
const modelCatalog = useModelCatalogStore()
const projects = useProjectsStore()
const form = useSettingsForm()

const ENGINES: ModelEngine[] = ['claude', 'codex']
const tab = ref<ModelEngine>('claude')

// ── Load ──
const loadError = ref<string | null>(null)
const loaded = ref(false)
async function load() {
  loadError.value = null
  try {
    await appSettings.refresh()
    await modelCatalog.ensureLoaded(true)
    if (!projects.projects.length) await projects.fetchProjects()
    loaded.value = true
  } catch (e) {
    loadError.value = errorText(e, t)
  }
}
watch(() => props.active, (on) => {
  if (on) void load()
}, { immediate: true })

// ── Models of each engine, in display order (the order the list is saved in) ──
const models = computed<Record<ModelEngine, { id: string; description?: string }[]>>(() => {
  const rows = (opts: { value: string; description?: string }[]) =>
    opts.filter((o) => o.value).map((o) => ({ id: o.value, description: o.description }))
  return {
    claude: rows(MODEL_OPTIONS.claude),
    codex: rows(codexOptions(MODEL_OPTIONS.codex, modelCatalog.codexModels)),
  }
})
const ids = (e: ModelEngine) => models.value[e].map((m) => m.id)
const description = (id: string) => models.value[tab.value].find((m) => m.id === id)?.description
const saved = computed(() => parseEnabledModels(appSettings.settings?.enabled_models))
const verdicts = computed(() => ({ claude: modelCatalog.validationByModel, codex: {} }))
const blocksOf = (e: ModelEngine) => modelBlocks(ids(e), saved.value[e], verdicts.value[e])
const blocks = computed(() => blocksOf(tab.value))
const tabCount = (e: ModelEngine) => `${blocksOf(e).available.length}/${ids(e).length}`

// ── Draft: the rows whose switch differs from the saved list, per engine ──
const flipped = ref<Record<ModelEngine, Set<string>>>({ claude: new Set(), codex: new Set() })
const flippedIn = (e: ModelEngine) => new Set(ids(e).filter((id) => flipped.value[e].has(id)))
const changeCount = computed(() => ENGINES.reduce((n, e) => n + flippedIn(e).size, 0))
const isDirty = (id: string) => flipped.value[tab.value].has(id)
const isOn = (row: ModelRow) => row.usable && draftOn(saved.value[tab.value], flipped.value[tab.value], row.id)
function toggle(row: ModelRow) {
  if (!row.usable) return
  const set = flipped.value[tab.value]
  if (set.has(row.id)) set.delete(row.id)
  else set.add(row.id)
}
function discard() {
  flipped.value = { claude: new Set(), codex: new Set() }
}

// ── "In use" badges ──
const usage = computed(() => modelDefaultsUsage(appSettings.settings, projects.projects))
function badgesOf(id: string): { key: string; label: string; title: string }[] {
  const uses = usage.value[tab.value][id] ?? []
  const out = uses
    .filter((u) => u.place !== 'project')
    .map((u) => {
      const label = t(`settings.model.usage.${u.place}`)
      return { key: u.place, label, title: t('settings.model.usage.title', { place: label }) }
    })
  const inProjects = uses.filter((u) => u.place === 'project').map((u) => u.projectName ?? '')
  if (inProjects.length) {
    out.push({
      key: 'project',
      label: inProjects.length === 1
        ? t('settings.model.usage.project', { name: inProjects[0] })
        : t('settings.model.usage.projects', { n: inProjects.length }),
      title: t('settings.model.usage.title', { place: inProjects.join(', ') }),
    })
  }
  return out
}

// ── Scan ──
const scanning = computed(() => (tab.value === 'claude' ? modelCatalog.validating : modelCatalog.codexLoading))
const scannedAt = computed(() => (tab.value === 'claude' ? modelCatalog.validationCheckedAt : modelCatalog.codexRefreshedAt))
// "account Work" for a Claude account; Codex has one shared login, named by the backend.
const claudeAccount = (label: string) => (label ? t('settings.model.account', { name: label }) : '')
const account = computed(() =>
  tab.value === 'claude' ? claudeAccount(modelCatalog.validationAccountLabel) : modelCatalog.codexAccountLabel,
)
const scanError = computed(() => (tab.value === 'claude' ? modelCatalog.validationError : modelCatalog.codexError))
const progress = computed(() => modelCatalog.validationProgress)
// Never scanned and nothing saved: the empty state (every model is allowed, as before).
const neverScanned = computed(() => !scannedAt.value && !scanning.value && !scanError.value && saved.value[tab.value] === null)
const scanTip = computed(() =>
  tab.value === 'claude'
    ? t('settings.model.scanTipClaude', { count: CLAUDE_MODEL_IDS.length })
    : t('settings.model.scanTipCodex'),
)
function scan() {
  if (tab.value === 'claude') void modelCatalog.validateClaude()
  else void modelCatalog.refreshCodex()
}

/** "09:12 today", else the date and time. */
function fmtWhen(iso: string | null): string {
  if (!iso) return ''
  const d = new Date(iso)
  if (isNaN(d.getTime())) return ''
  const time = d.toLocaleTimeString(locale.value, { hour: '2-digit', minute: '2-digit' })
  if (d.toDateString() === new Date().toDateString()) return t('settings.model.today', { time })
  return d.toLocaleString(locale.value, { dateStyle: 'medium', timeStyle: 'short' })
}

// ── Save ──
const busy = ref(false)
const pending = ref<{ value: string; summary: { on: string[]; off: string[] }; changes: DefaultChange[] } | null>(null)

async function startSave() {
  if (!changeCount.value || busy.value || pending.value) return
  busy.value = true
  try {
    const lists = Object.fromEntries(
      ENGINES.map((e) => [e, engineListToSave(ids(e), saved.value[e], flippedIn(e))]),
    )
    const value = JSON.stringify(lists)
    const changes = await invoke<DefaultChange[]>('save_enabled_models', { value, dryRun: true })
    const summary = { on: [] as string[], off: [] as string[] }
    for (const e of ENGINES) {
      const s = draftSummary(ids(e), saved.value[e], flippedIn(e))
      summary.on.push(...s.on)
      summary.off.push(...s.off)
    }
    pending.value = { value, summary, changes }
  } catch (e) {
    toast.error(errorText(e, t))
  } finally {
    busy.value = false
  }
}

async function confirmSave() {
  if (!pending.value || busy.value) return
  busy.value = true
  try {
    await invoke('save_enabled_models', { value: pending.value.value, dryRun: false })
    pending.value = null
    discard()
    // The save may have moved defaults: reload everything that shows them.
    await Promise.all([appSettings.refresh(), form.load(), projects.fetchProjects()])
    toast.success(t('settings.model.saved'))
  } catch (e) {
    toast.error(errorText(e, t))
  } finally {
    busy.value = false
  }
}

function openSectionFromDialog(id: SettingsSectionId) {
  pending.value = null
  emit('openSection', id)
}
function openProject(id: string) {
  pending.value = null
  const path = `/projects?${new URLSearchParams(projectPageQuery(id, 'session'))}`
  void emitEvent(SCREEN_OPEN_ROUTE, { path } satisfies ScreenOpenRoute)
}

// Project › Model (V3-64) narrows the list per project; the line shows once that section exists.
const hasProjectModels = sectionFromQuery('model') !== null
function openProjects() {
  void emitEvent(SCREEN_OPEN_ROUTE, { path: '/projects' } satisfies ScreenOpenRoute)
}

// ⌘S = Save, while this section is on screen.
function onKeydown(e: KeyboardEvent) {
  if (!props.active || !(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey || e.key.toLowerCase() !== 's') return
  if (!changeCount.value) return
  e.preventDefault()
  void startSave()
}
onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <div class="space-y-4">
    <div>
      <h2 class="text-base font-semibold text-foreground">{{ t('settings.sections.model') }}</h2>
      <p class="mt-1 text-sm text-muted-foreground">{{ t('settings.model.description') }}</p>
    </div>

    <Banner v-if="loadError" tone="destructive" live>
      <p class="font-medium">{{ t('settings.model.loadError') }}</p>
      <p class="break-words text-2xs opacity-80">{{ loadError }}</p>
      <template #actions>
        <Button variant="outline" size="xs" @click="load">{{ t('settings.model.retry') }}</Button>
      </template>
    </Banner>

    <div v-else-if="!loaded" class="h-28 animate-pulse rounded-lg border border-border bg-card" />

    <template v-else>
      <!-- Engine tabs: "Claude 5/8" = available / all -->
      <div class="flex flex-wrap items-end gap-1 border-b border-border" role="tablist">
        <button
          v-for="e in ENGINES"
          :key="e"
          type="button"
          role="tab"
          :aria-selected="tab === e"
          class="-mb-px inline-flex h-9 cursor-pointer items-center gap-1.5 border-b-2 px-2.5 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          :class="tab === e ? 'border-primary text-foreground' : 'border-transparent text-muted-foreground hover:text-foreground'"
          @click="tab = e"
        >
          {{ e === 'claude' ? 'Claude' : 'Codex' }}
          <span class="text-2xs font-normal tabular-nums text-muted-foreground">{{ tabCount(e) }}</span>
        </button>
      </div>

      <!-- Never scanned: nothing to show beyond "everything is allowed" -->
      <div
        v-if="neverScanned"
        class="flex flex-col items-center gap-1.5 rounded-lg border border-dashed border-input px-4 py-7 text-center"
      >
        <ScanSearch class="h-5 w-5 text-muted-foreground" :stroke-width="1.75" />
        <p class="text-sm font-medium">{{ t('settings.model.neverTitle') }}</p>
        <p class="max-w-[54ch] text-2xs text-muted-foreground">{{ t('settings.model.neverHint') }}</p>
        <Button class="mt-1.5" :title="scanTip" @click="scan">
          <ScanSearch class="h-3.5 w-3.5" :stroke-width="1.75" />{{ t('settings.model.scan') }}
        </Button>
      </div>

      <template v-else>
        <!-- Scan line -->
        <div class="flex min-h-8 flex-wrap items-center gap-2">
          <span v-if="scanning" class="inline-flex items-center gap-1.5 text-2xs text-muted-foreground" role="status">
            <Loader2 class="h-4 w-4 animate-spin" :stroke-width="1.75" />
            <template v-if="tab === 'claude' && progress">
              {{ t('settings.model.scanningClaude', { done: progress.done, total: progress.total, account: claudeAccount(progress.account_label) || account }) }}
            </template>
            <template v-else>{{ t('settings.model.scanningCodex') }}</template>
          </span>
          <span v-else class="inline-flex items-center gap-1.5 text-2xs text-muted-foreground">
            <History class="h-4 w-4" :stroke-width="1.75" />
            {{ scannedAt ? t('settings.model.scannedAt', { when: fmtWhen(scannedAt) }) : t('settings.model.notScanned') }}<template v-if="account"> · {{ account }}</template>
          </span>
          <span class="flex-1" />
          <Button variant="outline" size="xs" :disabled="scanning" :title="scanTip" @click="scan">
            <component :is="scanning ? Loader2 : ScanSearch" class="h-3.5 w-3.5" :class="scanning && 'animate-spin'" :stroke-width="1.75" />
            {{ scanning ? t('settings.model.scanning') : t('settings.model.scan') }}
          </Button>
        </div>
        <div
          v-if="scanning && tab === 'claude' && progress"
          class="-mt-2 h-1 overflow-hidden rounded-sm bg-muted"
          role="progressbar"
          :aria-valuenow="progress.done"
          :aria-valuemax="progress.total"
        >
          <span class="block h-full bg-primary transition-all" :style="{ width: `${progress.total ? (progress.done / progress.total) * 100 : 0}%` }" />
        </div>

        <Banner v-if="scanError && !scanning" tone="destructive" live>
          <span class="font-medium">{{ t('settings.model.scanFailed') }}</span>
          {{ scannedAt
            ? t('settings.model.scanFailedPrev', { when: fmtWhen(scannedAt), error: scanError })
            : t('settings.model.scanFailedNoPrev', { error: scanError }) }}
          <template #actions>
            <Button variant="outline" size="xs" @click="scan">
              <RotateCw class="h-3.5 w-3.5" :stroke-width="1.75" />{{ t('settings.model.retry') }}
            </Button>
          </template>
        </Banner>

        <p v-if="tab === 'codex' && saved.codex !== null" class="text-2xs text-muted-foreground">{{ t('settings.model.codexNewOff') }}</p>

        <!-- Available / Not available, from the saved list -->
        <section
          v-for="b in (['available', 'unavailable'] as const)"
          :key="b"
          class="space-y-1.5"
        >
          <div class="flex flex-wrap items-baseline gap-x-2.5 gap-y-0.5">
            <h3 class="inline-flex items-baseline gap-1.5 text-sm font-semibold">
              {{ t(`settings.model.${b}`) }}
              <span class="text-2xs font-normal tabular-nums text-muted-foreground">{{ blocks[b].length }}</span>
            </h3>
            <span class="text-2xs text-muted-foreground">{{ t(`settings.model.${b}Hint`) }}</span>
          </div>
          <div class="divide-y divide-border rounded-lg border border-border bg-card">
            <p v-if="!blocks[b].length" class="p-3 text-2xs text-muted-foreground">{{ t(`settings.model.${b}Empty`) }}</p>
            <div
              v-for="row in blocks[b]"
              :key="row.id"
              class="relative flex min-h-12 min-w-0 items-center gap-2 px-3 py-1.5"
              :class="isDirty(row.id) && 'before:absolute before:left-0 before:inset-y-1.5 before:w-0.5 before:rounded-r-sm before:bg-warning'"
            >
              <span class="min-w-0 flex-1">
                <span class="block truncate font-mono text-sm" :title="row.id">{{ row.id }}</span>
                <span v-if="!row.usable && row.reason" class="flex items-start gap-1 break-words text-2xs text-warning">
                  <AlertCircle class="mt-0.5 h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />{{ row.reason }}
                </span>
                <span v-else-if="description(row.id)" class="block text-2xs text-muted-foreground">{{ description(row.id) }}</span>
              </span>
              <Badge v-for="u in badgesOf(row.id)" :key="u.key" :title="u.title">{{ u.label }}</Badge>
              <Badge v-if="!row.usable" tone="warning">{{ t('settings.model.unusable') }}</Badge>
              <span
                v-if="isDirty(row.id)"
                class="inline-flex h-[22px] shrink-0 items-center gap-1 whitespace-nowrap rounded-full border border-warning px-[7px] text-2xs font-medium text-warning"
              >{{ t('settings.model.unsaved') }}</span>
              <span
                v-if="!row.usable"
                class="inline-grid w-5 place-items-center text-muted-foreground"
                :title="t('settings.model.unusableTip')"
                :aria-label="t('settings.model.unusableTip')"
                role="img"
              ><Lock class="h-4 w-4" :stroke-width="1.75" /></span>
              <ToggleSwitch
                size="md"
                :checked="isOn(row)"
                :disabled="!row.usable"
                :label="t('settings.model.allow', { id: row.id })"
                @toggle="toggle(row)"
              />
            </div>
          </div>
        </section>
      </template>

      <p v-if="hasProjectModels" class="flex items-start gap-2 text-2xs text-muted-foreground">
        <Folder class="mt-0.5 h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
        <span>
          {{ t('settings.model.projectLine') }}
          <button type="button" class="cursor-pointer text-primary hover:underline" @click="openProjects">{{ t('settings.model.projectLink') }}</button>.
        </span>
      </p>

      <!-- Unsaved changes: sticky at the bottom of the panel -->
      <div
        v-if="changeCount"
        class="sticky bottom-3 z-[5] flex min-h-12 flex-wrap items-center gap-2 rounded-[10px] border border-border bg-card px-3.5 py-1.5"
      >
        <span class="inline-flex items-center gap-1.5 text-2xs font-medium text-warning" role="status">
          <CircleDot class="h-4 w-4" :stroke-width="1.75" />{{ t('settings.model.changes', { n: changeCount }, changeCount) }}
        </span>
        <span class="flex-1" />
        <Button variant="ghost" size="xs" :disabled="busy" @click="discard">{{ t('settings.model.cancel') }}</Button>
        <Button size="xs" :disabled="busy" :title="t('settings.model.saveTip')" @click="startSave">{{ t('settings.model.save.action') }}</Button>
      </div>
    </template>

    <ModelSaveDialog
      :open="!!pending"
      :summary="pending?.summary ?? { on: [], off: [] }"
      :changes="pending?.changes ?? []"
      :busy="busy"
      @cancel="pending = null"
      @confirm="confirmSave"
      @open-section="openSectionFromDialog"
      @open-project="openProject"
    />
  </div>
</template>
