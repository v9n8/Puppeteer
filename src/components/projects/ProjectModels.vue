<script setup lang="ts">
// Project › Model (V3-64): "Follow Settings" (the project's list is NULL) or "Choose
// for this project", a switch per model available in Settings › Model. The choice and
// the switches only change a draft until Save (⌘S). Save writes the list through
// `set_project_session_defaults` in one call with the project's default model when the
// new list turns that model off: it moves to the first model on in both lists, which
// the confirm dialog names first. The backend checks the pair again (AC-14).
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { RouterLink } from 'vue-router'
import { ArrowUpRight, Boxes, CircleDot, Info } from 'lucide-vue-next'
import { Button, Card, ToggleSwitch } from '@/components/ui'
import { useConfirm } from '@/composables/useConfirm'
import { useToast } from '@/composables/useToast'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { useProjectsStore, type Project } from '@/stores/projects'
import { MODEL_OPTIONS } from '@/lib/engineOptions'
import { composerDefaults, type ProjectSessionDefaults } from '@/lib/composerChips'
import {
  ALL_MODELS_ON, draftOn, engineListToSave, firstEnabled, intersectEnabled, isModelOff, projectModelRows,
  type EnabledModels, type ModelVerdict,
} from '@/lib/enabledModels'
import type { ModelEngine } from '@/lib/modelDefaultsUsage'
import type { ProjectSectionId } from '@/lib/projectSections'

const props = defineProps<{ project: Project }>()
const emit = defineEmits<{ open: [section: ProjectSectionId] }>()

const { t } = useI18n()
const { confirm } = useConfirm()
const { toast } = useToast()
const projectStore = useProjectsStore()
const appSettings = useAppSettingsStore()
const modelCatalog = useModelCatalogStore()

modelCatalog.ensureLoaded().catch(() => {})

const ENGINES: ModelEngine[] = ['claude', 'codex']
const ENGINE_LABEL: Record<ModelEngine, string> = { claude: 'Claude', codex: 'Codex' }
const SETTINGS_MODEL = '/settings/model'
const tab = ref<ModelEngine>('claude')

// ── Models of Settings › Model: the available ones, then those it turned off (locked) ──
const available = computed<Record<ModelEngine, { id: string; description?: string; locked: boolean }[]>>(() => {
  const rows = (engine: ModelEngine, opts: { value: string; description?: string }[], verdicts: Record<string, ModelVerdict> = {}) => {
    const all = opts.filter((o) => o.value)
    const description = new Map(all.map((o) => [o.value, o.description]))
    return projectModelRows(all.map((o) => o.value), modelCatalog.enabled[engine], verdicts)
      .map((r) => ({ id: r.id, description: description.get(r.id), locked: r.locked }))
  }
  return {
    claude: rows('claude', MODEL_OPTIONS.claude, modelCatalog.validationByModel),
    codex: rows('codex', modelCatalog.codexOptions(MODEL_OPTIONS.codex)),
  }
})
// The rows a project can switch: counts and the saved lists leave the locked ones out.
const ids = (e: ModelEngine) => available.value[e].filter((m) => !m.locked).map((m) => m.id)

// ── Saved state and draft ──
const own = computed<ProjectSessionDefaults>(() => props.project.session_defaults ?? {})
const saved = computed<EnabledModels | null>(() => own.value.enabled_models ?? null)
const savedMode = computed(() => (saved.value ? 'own' : 'follow'))
const mode = ref<'follow' | 'own'>(savedMode.value)
// Choosing for the project the first time starts with every available model on.
const base = computed<EnabledModels>(() => saved.value ?? ALL_MODELS_ON)
const flipped = ref<Record<ModelEngine, Set<string>>>({ claude: new Set(), codex: new Set() })
const flippedIn = (e: ModelEngine) => new Set(ids(e).filter((id) => flipped.value[e].has(id)))
const isOn = (e: ModelEngine, id: string) => draftOn(base.value[e], flipped.value[e], id)
const isDirty = (id: string) => flipped.value[tab.value].has(id)
const onCount = (e: ModelEngine) => `${ids(e).filter((id) => isOn(e, id)).length}/${ids(e).length}`
const changeCount = computed(() =>
  (mode.value !== savedMode.value ? 1 : 0)
  + (mode.value === 'own' ? ENGINES.reduce((n, e) => n + flippedIn(e).size, 0) : 0),
)

// The default model just moved by a save, per engine (the footer says so).
const moved = ref<Partial<Record<ModelEngine, string>>>({})

function discard() {
  flipped.value = { claude: new Set(), codex: new Set() }
  mode.value = savedMode.value
}
watch(() => props.project.id, () => {
  discard()
  moved.value = {}
})
watch(savedMode, (m) => {
  mode.value = m
})

function toggle(id: string) {
  const set = flipped.value[tab.value]
  if (set.has(id)) set.delete(id)
  else set.add(id)
  moved.value = {}
}
function pick(next: 'follow' | 'own') {
  mode.value = next
  moved.value = {}
}

// ── Footer: the default model a new session of this project starts with ──
const defaultModels = computed(() => composerDefaults({ settings: appSettings.settings, project: own.value }).models)
const modelName = (id: string) => id || t('settings.ai.modelDefault')

// ── Save ──
/** The lists to save: null = follow Settings. Unchanged engines keep their saved value. */
function draftLists(): EnabledModels | null {
  if (mode.value === 'follow') return null
  return {
    claude: engineListToSave(ids('claude'), base.value.claude, flippedIn('claude')),
    codex: engineListToSave(ids('codex'), base.value.codex, flippedIn('codex')),
  }
}

/** The project's own default models the new lists turn off, and where each moves. */
function movesFor(lists: EnabledModels | null): { engine: ModelEngine; from: string; to: string }[] {
  const on = intersectEnabled(modelCatalog.enabled, lists)
  return ENGINES.flatMap((engine) => {
    const from = (engine === 'claude' ? own.value.model_claude : own.value.model_codex)?.trim() ?? ''
    return from && isModelOff(engine, from, on) ? [{ engine, from, to: firstEnabled(engine, on) }] : []
  })
}

const busy = ref(false)
async function save() {
  if (!changeCount.value || busy.value) return
  const lists = draftLists()
  const moves = movesFor(lists)
  if (moves.length) {
    const message = moves.map((m) => t('projectDetail.model.confirmMove', { from: m.from, to: modelName(m.to) })).join('\n')
    const ok = await confirm({
      title: t('projectDetail.model.confirmTitle'),
      message,
      confirmLabel: t('projectDetail.model.save'),
      cancelLabel: t('projectDetail.model.cancel'),
      variant: 'primary',
    })
    if (!ok) return
  }
  const done = lists
    ? t('projectDetail.model.savedOwn', { project: props.project.name, claude: onCount('claude'), codex: onCount('codex') })
    : t('projectDetail.model.savedFollow', { project: props.project.name })
  const next: ProjectSessionDefaults = { ...own.value, enabled_models: lists }
  for (const m of moves) next[m.engine === 'claude' ? 'model_claude' : 'model_codex'] = m.to || null
  busy.value = true
  try {
    await projectStore.setSessionDefaults(props.project.id, next)
    flipped.value = { claude: new Set(), codex: new Set() }
    mode.value = savedMode.value
    moved.value = Object.fromEntries(moves.map((m) => [m.engine, m.to]))
    toast.success(done)
  } catch (e) {
    toast.error(String(e))
  } finally {
    busy.value = false
  }
}

// ⌘S = Save while this section is on screen (it is only mounted then).
function onKeydown(e: KeyboardEvent) {
  if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey || e.key.toLowerCase() !== 's') return
  if (!changeCount.value) return
  e.preventDefault()
  void save()
}
onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <div class="max-w-2xl space-y-4">
    <Card class="overflow-visible">
      <template #header>
        <Boxes class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('projectDetail.model.title') }}</span>
      </template>

      <!-- Follow Settings / Choose for this project; stacks when the pane is narrow -->
      <div class="flex flex-wrap gap-2 px-4 pb-1 pt-3" role="radiogroup" :aria-label="t('projectDetail.model.pickLabel')">
        <label
          v-for="o in (['follow', 'own'] as const)"
          :key="o"
          class="flex min-h-[52px] min-w-[17rem] flex-1 cursor-pointer items-start gap-2 rounded-lg border px-3 py-2 text-2xs transition-colors hover:bg-accent focus-within:ring-2 focus-within:ring-ring"
          :class="mode === o ? 'border-primary' : 'border-border'"
        >
          <input
            type="radio"
            name="project-model-mode"
            class="mt-0.5 h-4 w-4 shrink-0 accent-primary focus-visible:outline-none"
            :checked="mode === o"
            @change="pick(o)"
          >
          <span class="min-w-0">
            <b class="block text-sm font-semibold">{{ t(`projectDetail.model.${o}`) }}</b>
            <span class="block text-muted-foreground">{{ t(`projectDetail.model.${o}Hint`) }}</span>
          </span>
        </label>
      </div>

      <!-- Follow Settings: what Settings offers now -->
      <div v-if="mode === 'follow'" class="space-y-3 px-4 pb-4 pt-2">
        <div v-for="e in ENGINES" :key="e" class="space-y-1.5">
          <p class="text-2xs font-medium text-muted-foreground">
            {{ t('projectDetail.model.engineCount', { engine: ENGINE_LABEL[e], n: ids(e).length }, ids(e).length) }}
          </p>
          <span v-if="ids(e).length" class="flex flex-wrap gap-1.5">
            <span
              v-for="id in ids(e)"
              :key="id"
              class="inline-flex h-[22px] items-center rounded-full border border-border px-2 font-mono text-2xs"
            >{{ id }}</span>
          </span>
          <p v-else class="text-2xs text-muted-foreground">{{ t('projectDetail.model.noneAvailable') }}</p>
        </div>
        <RouterLink :to="SETTINGS_MODEL" class="inline-flex min-h-7 items-center gap-1 text-2xs text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
          {{ t('projectDetail.model.openSettings') }}<ArrowUpRight class="h-3.5 w-3.5" :stroke-width="1.75" />
        </RouterLink>
      </div>

      <!-- Choose for this project: a switch per model available in Settings -->
      <template v-else>
        <p class="flex flex-wrap items-center gap-1.5 px-4 py-2 text-2xs text-muted-foreground">
          <Info class="h-4 w-4 shrink-0" :stroke-width="1.75" />
          <span class="flex-1">{{ t('projectDetail.model.offHidden') }}</span>
          <RouterLink :to="SETTINGS_MODEL" class="inline-flex min-h-7 items-center gap-1 text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
            {{ t('projectDetail.model.openSettings') }}<ArrowUpRight class="h-3.5 w-3.5" :stroke-width="1.75" />
          </RouterLink>
        </p>
        <div class="flex flex-wrap items-end gap-1 border-b border-border px-2" role="tablist" :aria-label="t('projectDetail.model.engineTabs')">
          <button
            v-for="e in ENGINES"
            :key="e"
            type="button"
            role="tab"
            :aria-selected="tab === e"
            :title="t('projectDetail.model.tabTitle', { engine: ENGINE_LABEL[e], count: onCount(e) })"
            class="-mb-px inline-flex h-9 cursor-pointer items-center gap-1.5 border-b-2 px-2.5 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            :class="tab === e ? 'border-primary text-foreground' : 'border-transparent text-muted-foreground hover:text-foreground'"
            @click="tab = e"
          >
            {{ ENGINE_LABEL[e] }}
            <span class="text-2xs font-normal tabular-nums text-muted-foreground">{{ onCount(e) }}</span>
          </button>
        </div>
        <div class="divide-y divide-border">
          <p v-if="!available[tab].length" class="px-4 py-6 text-center text-2xs text-muted-foreground">{{ t('projectDetail.model.noneAvailable') }}</p>
          <div
            v-for="m in available[tab]"
            :key="m.id"
            class="relative flex min-h-12 min-w-0 items-center gap-2 px-4 py-1.5 hover:bg-accent/50"
            :class="isDirty(m.id) && 'before:absolute before:inset-y-1.5 before:left-0 before:w-0.5 before:rounded-r-sm before:bg-warning'"
          >
            <span class="min-w-0 flex-1">
              <span class="flex flex-wrap items-center gap-1.5">
                <span class="truncate font-mono text-sm" :title="m.id">{{ m.id }}</span>
                <span
                  v-if="isDirty(m.id)"
                  class="inline-flex h-[22px] shrink-0 items-center whitespace-nowrap rounded-full border border-warning px-[7px] text-2xs font-medium text-warning"
                >{{ t('projectDetail.model.unsaved') }}</span>
              </span>
              <span v-if="m.description" class="block text-2xs text-muted-foreground">{{ m.description }}</span>
            </span>
            <ToggleSwitch
              size="md"
              :checked="!m.locked && isOn(tab, m.id)"
              :disabled="m.locked"
              :label="m.locked ? t('projectDetail.model.lockedAppWide', { id: m.id }) : t(isOn(tab, m.id) ? 'projectDetail.model.dontUse' : 'projectDetail.model.use', { id: m.id })"
              @toggle="toggle(m.id)"
            />
          </div>
        </div>
        <p class="border-t border-border px-4 pb-3 pt-2.5 text-2xs leading-relaxed text-muted-foreground">
          {{ t('projectDetail.model.footBefore') }}
          <button
            type="button"
            class="min-h-7 cursor-pointer text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            @click="emit('open', 'session')"
          >{{ t('projectDetail.model.footLink') }}</button>
          {{ t('projectDetail.model.footAfter') }}<template v-if="moved[tab] !== undefined">{{ t('projectDetail.model.footMoved', { id: modelName(moved[tab]!) }) }}</template><template v-else-if="defaultModels[tab]">{{ t('projectDetail.model.footNow', { id: defaultModels[tab] }) }}</template>.
        </p>
      </template>

      <!-- Unsaved changes: sticky at the bottom of the card -->
      <div
        v-if="changeCount"
        class="sticky bottom-0 z-[2] flex min-h-11 flex-wrap items-center gap-2 rounded-b-lg border-t border-border bg-muted px-3.5 py-1.5"
      >
        <span class="inline-flex flex-1 items-center gap-1.5 text-2xs font-medium text-warning" role="status">
          <CircleDot class="h-4 w-4" :stroke-width="1.75" />{{ t('projectDetail.model.changes', { n: changeCount }, changeCount) }}
        </span>
        <Button variant="ghost" size="xs" :disabled="busy" @click="discard">{{ t('projectDetail.model.cancel') }}</Button>
        <Button size="xs" :disabled="busy" :title="t('projectDetail.model.saveTip')" @click="save">{{ t('projectDetail.model.save') }}</Button>
      </div>
    </Card>
  </div>
</template>
