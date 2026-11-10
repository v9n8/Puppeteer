<script setup lang="ts">
// Settings → "Defaults for new sessions": engine, the model of each engine,
// permission mode, effort and response style a new session starts with. The one
// place these keys are edited; the engine cards keep the model lists (validate /
// refresh) and link here. Translation has its own section (TranslateSettings).
import { computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { ShieldAlert, Sparkles } from 'lucide-vue-next'
import { AppSelect, Card } from '@/components/ui'
import { EFFORT_LEVELS, effortLevelsFor, type SelectOption } from '@/lib/engineOptions'
import { builtinStyleLabelKey } from '@/lib/responseStyles'
import { defaultEffortChoices, defaultStyleChoices } from '@/lib/sessionDefaults'
import type { SettingsSectionId } from '@/lib/settingsSections'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { useResponseStylesStore } from '@/stores/responseStyles'
import { useSettingsForm } from './useSettingsForm'
import { useModelOptions } from './useModelOptions'

const emit = defineEmits<{ openSection: [id: SettingsSectionId] }>()

const { t } = useI18n()
const { settings } = useSettingsForm()
const modelCatalog = useModelCatalogStore()
const styles = useResponseStylesStore()
const { claudeModelOptions, codexModelOptions } = useModelOptions()

onMounted(() => {
  // My styles; Settings › Response styles loads the same list.
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

// Built-ins and My styles under their own headings.
const styleOptions = computed(() => {
  const out: (SelectOption & { header?: boolean })[] = []
  let group = ''
  const heading = (id: string, label: string) => {
    if (group === id) return
    group = id
    out.push({ value: `__group:${id}`, label, header: true })
  }
  for (const c of defaultStyleChoices(styles.styles, settings.value.default_style)) {
    if (c.kind === 'none') {
      out.push({ value: '', label: t('settings.ai.styleNone'), description: t('settings.ai.styleNoneHint') })
    } else if (c.kind === 'builtin') {
      heading('builtin', t('responseStyles.builtinTitle'))
      out.push({ value: c.value, label: t(builtinStyleLabelKey(c.value)), description: t(`responseStyles.builtin.${c.value}.description`) })
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
    </template>
      <p class="text-2xs text-muted-foreground leading-relaxed">{{ t('settings.ai.sessionDefaultsHint') }}</p>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.defaultEngine') }}</label>
        <AppSelect
          size="sm"
          v-model="settings.default_engine"
          :options="[
            { value: 'claude', label: 'claude' },
            { value: 'codex', label: 'codex' },
          ]"
        />
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.defaultClaudeModel') }}</label>
        <AppSelect size="sm" v-model="settings.claude_model" :options="claudeModelOptions" />
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.defaultCodexModel') }}</label>
        <AppSelect size="sm" v-model="settings.codex_model" :options="codexModelOptions" />
        <p class="text-2xs text-muted-foreground leading-relaxed">
          {{ t('settings.ai.modelListsHint') }}
          <button type="button" class="text-primary hover:underline cursor-pointer" @click="emit('openSection', 'engines')">{{ t('settings.sections.engines') }}</button>
        </p>
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
          <ShieldAlert class="h-3 w-3" :stroke-width="1.75" />{{ t('settings.ai.defaultPermissionMode') }}
        </label>
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
        <p class="text-2xs text-muted-foreground leading-relaxed">
          {{ t('settings.ai.permissionHint') }}
        </p>
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.defaultEffort') }}</label>
        <template v-if="effortLevels.length">
          <AppSelect size="sm" v-model="settings.default_effort" :options="effortOptions" />
          <p class="text-2xs text-muted-foreground leading-relaxed">
            {{ t('settings.ai.effortHint') }}
          </p>
        </template>
        <p v-else class="text-2xs text-muted-foreground leading-relaxed">{{ t('settings.ai.effortNotTaken') }}</p>
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.defaultStyle') }}</label>
        <AppSelect size="sm" v-model="settings.default_style" :options="styleOptions" />
        <p class="text-2xs text-muted-foreground leading-relaxed">
          {{ t('settings.ai.styleHint') }}
          <button type="button" class="text-primary hover:underline cursor-pointer" @click="emit('openSection', 'styles')">{{ t('settings.sections.styles') }}</button>
        </p>
      </div>
  </Card>
</template>
