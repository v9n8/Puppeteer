<script setup lang="ts">
// Settings → "Translation": engine, model, target language and style used to
// translate highlighted text (`translate_*` keys).
import { computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Languages } from 'lucide-vue-next'
import { AppSelect, Card } from '@/components/ui'
import { useSettingsForm } from './useSettingsForm'
import { useModelOptions } from './useModelOptions'

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
    </template>

      <p class="text-2xs text-muted-foreground leading-relaxed">
        {{ t('settings.translate.hint') }}
      </p>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.translate.engine') }}</label>
        <AppSelect
          size="sm"
          v-model="settings.translate_engine"
          :options="[
            { value: 'claude', label: 'claude' },
            { value: 'codex', label: 'codex' },
          ]"
        />
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.translate.model') }}</label>
        <AppSelect size="sm" v-model="settings.translate_model" :options="translateModelOptions" />
        <p class="text-2xs text-muted-foreground">{{ t('settings.translate.modelHint') }}</p>
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.translate.targetLanguage') }}</label>
        <AppSelect size="sm" v-model="settings.translate_target_lang" :options="TRANSLATE_TARGET_OPTIONS" />
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.translate.style') }}</label>
        <AppSelect size="sm" v-model="settings.translate_style" :options="TRANSLATE_STYLE_OPTIONS" />
      </div>
  </Card>
</template>
