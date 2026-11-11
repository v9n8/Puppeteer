<script setup lang="ts">
// Settings → "Translation": engine, model, target language and style used to
// translate highlighted text (`translate_*` keys).
import { computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Languages } from 'lucide-vue-next'
import { AppSelect, Card, InfoTip } from '@/components/ui'
import { useSettingsForm } from './useSettingsForm'
import { useModelOptions } from './useModelOptions'
import SettingsRow from './SettingsRow.vue'

const { t } = useI18n()
const { settings } = useSettingsForm()
const { claudeModelOptions, codexModelOptions } = useModelOptions()

// Translation model choices follow the chosen translation engine, reusing the
// same option tables (with cached models merged in) as the default-model selects.
const translateModelOptions = computed(() =>
  settings.value.translate_engine === 'codex' ? codexModelOptions.value : claudeModelOptions.value,
)

// Reset the translation model when switching to an engine that doesn't offer it.
watch(
  () => settings.value.translate_engine,
  () => {
    if (!translateModelOptions.value.some((o) => o.value === settings.value.translate_model)) {
      settings.value.translate_model = ''
    }
  },
)

const TRANSLATE_TARGET_OPTIONS = [
  { value: 'vi', label: 'Tiếng Việt' },
  { value: 'en', label: 'English' },
  { value: 'ja', label: '日本語 (Japanese)' },
  { value: 'zh', label: '中文 (Chinese)' },
  { value: 'ko', label: '한국어 (Korean)' },
]
const TRANSLATE_STYLE_OPTIONS = computed(() => [
  { value: 'natural', label: t('settings.translate.styleNatural') },
  { value: 'literal', label: t('settings.translate.styleLiteral') },
  { value: 'technical', label: t('settings.translate.styleTechnical') },
  { value: 'formal', label: t('settings.translate.styleFormal') },
  { value: 'casual', label: t('settings.translate.styleCasual') },
])
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Languages class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.translate.title') }}</span>
      <InfoTip :text="t('settings.translate.hint')" />
    </template>

      <SettingsRow :label="t('settings.translate.engine')">
        <AppSelect
          size="sm"
          v-model="settings.translate_engine"
          :options="[
            { value: 'claude', label: 'claude' },
            { value: 'codex', label: 'codex' },
          ]"
        />
      </SettingsRow>
      <SettingsRow :label="t('settings.translate.model')" :hint="t('settings.translate.modelHint')">
        <AppSelect size="sm" v-model="settings.translate_model" :options="translateModelOptions" />
      </SettingsRow>
      <SettingsRow :label="t('settings.translate.targetLanguage')">
        <AppSelect size="sm" v-model="settings.translate_target_lang" :options="TRANSLATE_TARGET_OPTIONS" />
      </SettingsRow>
      <SettingsRow :label="t('settings.translate.style')">
        <AppSelect size="sm" v-model="settings.translate_style" :options="TRANSLATE_STYLE_OPTIONS" />
      </SettingsRow>
  </Card>
</template>
