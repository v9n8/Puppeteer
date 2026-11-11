<script setup lang="ts">
// Settings → "Defaults for new sessions": engine, the model of each engine,
// permission mode, effort and response style a new session starts with. The one
// place these keys are edited; the engine cards keep the model lists (validate /
// refresh) and link here. Translation has its own section (TranslateSettings).
import { computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowUpRight, Sparkles } from 'lucide-vue-next'
import { AppSelect, Button, Card, InfoTip } from '@/components/ui'
import { EFFORT_LEVELS, effortLevelsFor, type SelectOption } from '@/lib/engineOptions'
import { shownStyles } from '@/lib/responseStyles'
import { defaultEffortChoices, defaultStyleChoices } from '@/lib/sessionDefaults'
import type { SettingsSectionId } from '@/lib/settingsSections'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { useResponseStylesStore } from '@/stores/responseStyles'
import { useSettingsForm } from './useSettingsForm'
import { useModelOptions } from './useModelOptions'
import SettingsRow from './SettingsRow.vue'

const emit = defineEmits<{ openSection: [id: SettingsSectionId] }>()

const { t } = useI18n()
const { settings } = useSettingsForm()
const modelCatalog = useModelCatalogStore()
const styles = useResponseStylesStore()
const { claudeModelOptions, claudeFallbackOptions, codexModelOptions } = useModelOptions()

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
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Sparkles class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.ai.sessionDefaults') }}</span>
      <InfoTip :text="t('settings.ai.sessionDefaultsHint')" />
    </template>
      <SettingsRow :label="t('settings.ai.defaultEngine')">
        <AppSelect
          size="sm"
          v-model="settings.default_engine"
          :options="[
            { value: 'claude', label: 'claude' },
            { value: 'codex', label: 'codex' },
          ]"
        />
      </SettingsRow>
      <SettingsRow :label="t('settings.ai.defaultClaudeModel')">
        <AppSelect size="sm" v-model="settings.claude_model" :options="claudeModelOptions" />
      </SettingsRow>
      <SettingsRow :label="t('settings.ai.claudeFallbackModel')" :hint="t('settings.ai.fallbackHint')">
        <AppSelect size="sm" v-model="settings.claude_fallback_model" :options="claudeFallbackOptions" />
      </SettingsRow>
      <SettingsRow :label="t('settings.ai.defaultCodexModel')" :hint="`${t('settings.ai.modelListsHint')} ${t('settings.sections.engines')}.`">
        <div class="flex items-center gap-1">
          <AppSelect size="sm" class="flex-1" v-model="settings.codex_model" :options="codexModelOptions" />
          <Button
            variant="ghost"
            size="icon"
            :title="t('settings.ai.openSection', { section: t('settings.sections.engines') })"
            :aria-label="t('settings.ai.openSection', { section: t('settings.sections.engines') })"
            @click="emit('openSection', 'engines')"
          >
            <ArrowUpRight class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
        </div>
      </SettingsRow>
      <SettingsRow :label="t('settings.ai.defaultPermissionMode')" :hint="t('settings.ai.permissionHint')">
        <AppSelect
          size="sm"
          v-model="settings.default_permission_mode"
          :options="[
            { value: 'default', label: t('settings.ai.permAskUi') },
            { value: 'acceptEdits', label: t('settings.ai.permAcceptEdits') },
            { value: 'plan', label: t('settings.ai.permPlan') },
            { value: 'auto', label: t('settings.ai.permAuto') },
            { value: 'bypassPermissions', label: t('settings.ai.permBypass') },
          ]"
        />
      </SettingsRow>
      <SettingsRow :label="t('settings.ai.defaultEffort')" :hint="t('settings.ai.effortHint')">
        <AppSelect v-if="effortLevels.length" size="sm" v-model="settings.default_effort" :options="effortOptions" />
        <p v-else class="text-2xs text-muted-foreground leading-relaxed">{{ t('settings.ai.effortNotTaken') }}</p>
      </SettingsRow>
      <SettingsRow :label="t('settings.ai.defaultStyle')" :hint="`${t('settings.ai.styleHint')} ${t('settings.sections.styles')}.`">
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
  </Card>
</template>
