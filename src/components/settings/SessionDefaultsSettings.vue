<script setup lang="ts">
// Settings → "Defaults for new sessions": engine, the model of each engine,
// permission mode, effort, response style, worker cap and effort, and the model
// and effort of its sub-agents a new session starts with. The one
// place these keys are edited; Settings › Model scans the model lists and picks
// the models offered here. Translation has its own section (TranslateSettings).
import { computed, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowUpRight, Sparkles } from 'lucide-vue-next'
import { AppSelect, Button, Card, InfoTip, Input } from '@/components/ui'
import { EFFORT_LEVELS, effortLevelsFor, permissionModeOptions, type SelectOption } from '@/lib/engineOptions'
import { shownStyles } from '@/lib/responseStyles'
import { INHERIT, SUBAGENT_EFFORT_LEVELS, defaultEffortChoices, defaultStyleChoices, inheritEffortChoices } from '@/lib/sessionDefaults'
import { DEFAULT_WORKER_CAP, MAX_WORKER_CAP, MIN_WORKER_CAP, defaultWorkerCap } from '@/lib/workerCap'
import type { SettingsSectionId } from '@/lib/settingsSections'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { useResponseStylesStore } from '@/stores/responseStyles'
import { useSettingsForm } from './useSettingsForm'
import { useModelOptions } from './useModelOptions'
import SettingsRow from './SettingsRow.vue'
import SettingsSectionHead from './SettingsSectionHead.vue'

const emit = defineEmits<{ openSection: [id: SettingsSectionId] }>()

const { t } = useI18n()
const { settings } = useSettingsForm()
const modelCatalog = useModelCatalogStore()
const styles = useResponseStylesStore()
const { claudeModelOptions, claudeFallbackOptions, claudeSubagentOptions, codexModelOptions } = useModelOptions()

onMounted(() => {
  // The global styles; Settings › Response styles loads the same list.
  if (!styles.loaded) styles.load(null).catch(() => {})
})

// Effort levels of the default engine + its default model (empty: it takes none).
const effortLevels = computed(() => {
  const engine = settings.value.default_engine
  const model = engine === 'codex' ? settings.value.codex_model : settings.value.claude_model
  return effortLevelsFor(engine, model, { codex: modelCatalog.codexModels })
})
const effortOptions = computed<SelectOption[]>(() =>
  defaultEffortChoices(effortLevels.value, settings.value.default_effort).map((c) => {
    if (!c.value) return { value: '', label: t('settings.ai.effortEngineDefault') }
    const label = (EFFORT_LEVELS as readonly string[]).includes(c.value) ? t(`settings.ai.effort.${c.value}`) : c.value
    return c.supported ? { value: c.value, label } : { value: c.value, label, description: t('settings.ai.effortUnsupported') }
  }),
)

// Worker and sub-agent effort: `inherit` ("same as …") first, then the levels.
const levelLabel = (v: string) => ((EFFORT_LEVELS as readonly string[]).includes(v) ? t(`settings.ai.effort.${v}`) : v)
const inheritOptions = (levels: readonly string[], current: string, sameLabel: string): SelectOption[] =>
  inheritEffortChoices(levels, current).map((v) => ({ value: v, label: v === INHERIT ? sameLabel : levelLabel(v) }))
const workerEffortOptions = computed(() =>
  inheritOptions(EFFORT_LEVELS, settings.value.worker_effort, t('settings.ai.sameAsNewSessions')),
)
const subagentEffortOptions = computed(() =>
  inheritOptions(SUBAGENT_EFFORT_LEVELS, settings.value.claude_subagent_effort, t('settings.ai.sameAsSession')),
)

// The worker cap is typed into a draft and saved clamped once the field is left,
// so "0" or "500" is stored as 1 or 100 and a half-typed number never auto-saves.
const maxWorkersDraft = ref('')
watch(() => settings.value.default_max_workers, (v) => { maxWorkersDraft.value = v }, { immediate: true })
function commitMaxWorkers() {
  settings.value.default_max_workers = String(defaultWorkerCap(maxWorkersDraft.value))
  maxWorkersDraft.value = settings.value.default_max_workers
}

// The global styles under their heading.
const styleOptions = computed(() => {
  const out: (SelectOption & { header?: boolean })[] = []
  let group = ''
  const heading = (id: string, label: string) => {
    if (group === id) return
    group = id
    out.push({ value: `__group:${id}`, label, header: true })
  }
  for (const c of defaultStyleChoices(shownStyles(styles.styles, t), settings.value.default_style)) {
    if (c.kind === 'none') {
      out.push({ value: c.value, label: t('settings.ai.styleNone'), description: t('settings.ai.styleNoneHint') })
    } else if (c.kind === 'mine') {
      heading('mine', t('responseStyles.mineTitle'))
      out.push(c.description ? { value: c.value, label: c.name, description: c.description } : { value: c.value, label: c.name })
    } else {
      // Before the list loads a style id cannot be named yet; after, it is gone.
      out.push({ value: c.value, label: styles.loaded ? t('settings.ai.styleMissing') : '…' })
    }
  }
  return out
})
</script>

<template>
  <div class="space-y-4">
  <SettingsSectionHead :title="t('settings.sections.ai')" />
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Sparkles class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.ai.sessionDefaults') }}</span>
      <InfoTip :text="t('settings.ai.sessionDefaultsHint')" />
    </template>
      <SettingsRow search-id="ai-defaultEngine" :label="t('settings.ai.defaultEngine')">
        <AppSelect
          size="sm"
          v-model="settings.default_engine"
          :options="[
            { value: 'claude', label: 'claude' },
            { value: 'codex', label: 'codex' },
          ]"
        />
      </SettingsRow>
      <SettingsRow search-id="ai-defaultClaudeModel" :label="t('settings.ai.defaultClaudeModel')">
        <AppSelect size="sm" v-model="settings.claude_model" :options="claudeModelOptions" />
      </SettingsRow>
      <SettingsRow search-id="ai-claudeFallbackModel" :label="t('settings.ai.claudeFallbackModel')" :hint="t('settings.ai.fallbackHint')">
        <AppSelect size="sm" v-model="settings.claude_fallback_model" :options="claudeFallbackOptions" />
      </SettingsRow>
      <SettingsRow search-id="ai-defaultCodexModel" :label="t('settings.ai.defaultCodexModel')" :hint="`${t('settings.ai.modelListsHint')} ${t('settings.sections.model')}.`">
        <div class="flex items-center gap-1">
          <AppSelect size="sm" class="flex-1" v-model="settings.codex_model" :options="codexModelOptions" />
          <Button
            variant="ghost"
            size="icon"
            :title="t('settings.ai.openSection', { section: t('settings.sections.model') })"
            :aria-label="t('settings.ai.openSection', { section: t('settings.sections.model') })"
            @click="emit('openSection', 'model')"
          >
            <ArrowUpRight class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
        </div>
      </SettingsRow>
      <SettingsRow search-id="ai-defaultPermissionMode" :label="t('settings.ai.defaultPermissionMode')" :hint="t('settings.ai.permissionHint')">
        <AppSelect
          size="sm"
          v-model="settings.default_permission_mode"
          :options="permissionModeOptions(t)"
        />
      </SettingsRow>
      <SettingsRow search-id="ai-defaultEffort" :label="t('settings.ai.defaultEffort')" :hint="t('settings.ai.effortHint')">
        <AppSelect v-if="effortLevels.length" size="sm" v-model="settings.default_effort" :options="effortOptions" />
        <p v-else class="text-2xs text-muted-foreground leading-relaxed">{{ t('settings.ai.effortNotTaken') }}</p>
      </SettingsRow>
      <SettingsRow search-id="ai-defaultStyle" :label="t('settings.ai.defaultStyle')" :hint="`${t('settings.ai.styleHint')} ${t('settings.sections.styles')}.`">
        <div class="flex items-center gap-1">
          <AppSelect size="sm" class="flex-1" v-model="settings.default_style" :options="styleOptions" />
          <Button
            variant="ghost"
            size="icon"
            :title="t('settings.ai.openSection', { section: t('settings.sections.styles') })"
            :aria-label="t('settings.ai.openSection', { section: t('settings.sections.styles') })"
            @click="emit('openSection', 'styles')"
          >
            <ArrowUpRight class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
        </div>
      </SettingsRow>
      <SettingsRow search-id="ai-defaultMaxWorkers" :label="t('settings.ai.defaultMaxWorkers')" :hint="t('settings.ai.maxWorkersHint')">
        <Input
          v-model="maxWorkersDraft"
          type="number"
          :min="MIN_WORKER_CAP"
          :max="MAX_WORKER_CAP"
          :placeholder="String(DEFAULT_WORKER_CAP)"
          @change="commitMaxWorkers"
        />
      </SettingsRow>
      <SettingsRow search-id="ai-workerEffort" :label="t('settings.ai.workerEffort')" :hint="t('settings.ai.workerEffortHint')">
        <AppSelect size="sm" v-model="settings.worker_effort" :options="workerEffortOptions" />
      </SettingsRow>
      <SettingsRow search-id="ai-subagentModel" :label="t('settings.ai.subagentModel')" :hint="t('settings.ai.subagentModelHint')">
        <AppSelect size="sm" v-model="settings.claude_subagent_model" :options="claudeSubagentOptions" />
      </SettingsRow>
      <SettingsRow search-id="ai-subagentEffort" :label="t('settings.ai.subagentEffort')" :hint="t('settings.ai.subagentEffortHint')">
        <AppSelect size="sm" v-model="settings.claude_subagent_effort" :options="subagentEffortOptions" />
      </SettingsRow>
  </Card>
  </div>
</template>
