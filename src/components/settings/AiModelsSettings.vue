<script setup lang="ts">
// Settings → "AI & Models": run defaults (engine, models, permission mode) and
// translation of highlighted text, grouped together.
import { computed, onMounted, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { BadgeCheck, Cpu, Loader2, RefreshCw, ShieldAlert, Sparkles } from 'lucide-vue-next'
import { AppSelect, Card } from '@/components/ui'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { MODEL_OPTIONS, withCurrentModel } from '@/lib/engineOptions'
import { useSettingsForm } from './useSettingsForm'

const { t } = useI18n()
const modelCatalog = useModelCatalogStore()
const { settings } = useSettingsForm()

// Claude model choices = the pinned table, annotated with the last validation
// sweep, plus whatever is already saved (so an id dropped from the table — or a
// family alias from an older build — is never silently rewritten).
const claudeModelOptions = computed(() =>
  modelCatalog.withValidation(
    withCurrentModel(CLAUDE_MODEL_OPTIONS, settings.value.claude_model),
  ),
)
// Codex model choices = curated fallback, replaced by the CLI-refreshed cache
// (`codex debug models`) once the user refreshes in Settings.
const codexModelOptions = computed(() => modelCatalog.codexOptions(CODEX_MODEL_OPTIONS))

// Translation model choices follow the chosen translation engine, reusing the
// same option tables (with cached models merged in) as the selectors.
const translateModelOptions = computed(() =>
  settings.value.translate_engine === 'codex' ? codexModelOptions.value : claudeModelOptions.value,
)

/** Format a cache timestamp for the "last refreshed" hint. */
function fmtRefreshed(iso: string | null | undefined): string {
  if (!iso) return ''
  const d = new Date(iso)
  return isNaN(d.getTime()) ? '' : d.toLocaleString()
}
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
  { value: 'natural', label: t('settings.ai.styleNatural') },
  { value: 'literal', label: t('settings.ai.styleLiteral') },
  { value: 'technical', label: t('settings.ai.styleTechnical') },
  { value: 'formal', label: t('settings.ai.styleFormal') },
  { value: 'casual', label: t('settings.ai.styleCasual') },
])

// The pinned Claude table, shared with the composer and the remote controller.
// Only the leading "Default" placeholder is translated; the rest are model ids.
const CLAUDE_MODEL_OPTIONS = MODEL_OPTIONS.claude.map((o) =>
  o.value === '' ? { ...o, label: t('settings.ai.modelDefault') } : o,
)
const CODEX_MODEL_OPTIONS = [
  { value: '', label: t('settings.ai.modelDefault') },
  { value: 'gpt-5.5', label: 'gpt-5.5' },
  { value: 'gpt-5.4', label: 'gpt-5.4' },
  { value: 'gpt-5.3-codex', label: 'gpt-5.3-codex' },
  { value: 'gpt-5.2-codex', label: 'gpt-5.2-codex' },
  { value: 'gpt-5.1-codex-mini', label: 'gpt-5.1-codex-mini' },
]

onMounted(() => {
  // Hydrate the persisted model caches (no discovery — refresh is manual).
  modelCatalog.ensureLoaded().catch(() => {})
})
</script>

<template>
  <Card body-class="p-4 space-y-5">
    <template #header>
      <Sparkles class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.ai.title') }}</span>
    </template>

      <!-- Group 1: run defaults -->
      <div class="space-y-4">
        <div class="flex items-center gap-1.5 text-muted-foreground">
          <Cpu class="h-3 w-3" :stroke-width="1.75" />
          <span class="text-2xs font-semibold uppercase tracking-wider">{{ t('settings.ai.runDefaults') }}</span>
        </div>
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
          <div class="flex items-center justify-between">
            <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.defaultClaudeModel') }}</label>
            <button
              class="inline-flex items-center gap-1 text-2xs text-muted-foreground hover:text-foreground transition-colors cursor-pointer disabled:opacity-50"
              :disabled="modelCatalog.validating"
              :title="t('settings.ai.validateModelsTitle')"
              @click="modelCatalog.validateClaude()"
            >
              <component :is="modelCatalog.validating ? Loader2 : BadgeCheck" class="h-3 w-3" :class="{ 'animate-spin': modelCatalog.validating }" :stroke-width="1.75" />
              {{ modelCatalog.validating ? t('settings.ai.validating') : t('settings.ai.validate') }}
            </button>
          </div>
          <AppSelect size="sm" v-model="settings.claude_model" :options="claudeModelOptions" />
          <p v-if="modelCatalog.validationError" class="text-2xs text-warning">{{ t('settings.ai.validateError') }}</p>
          <p v-else-if="modelCatalog.validating" class="text-2xs text-muted-foreground">{{ t('settings.ai.validateRunning') }}</p>
          <p v-else-if="modelCatalog.unsupportedModels.length" class="text-2xs text-warning">{{ t('settings.ai.validateSomeFailed', { models: modelCatalog.unsupportedModels.join(', '), when: fmtRefreshed(modelCatalog.validationCheckedAt) }) }}</p>
          <p v-else-if="modelCatalog.validationCheckedAt" class="text-2xs text-muted-foreground">{{ t('settings.ai.validateAllOk', { when: fmtRefreshed(modelCatalog.validationCheckedAt) }) }}</p>
          <p v-else class="text-2xs text-muted-foreground">{{ t('settings.ai.validateHint') }}</p>
        </div>
        <div class="space-y-1.5">
          <div class="flex items-center justify-between">
            <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.defaultCodexModel') }}</label>
            <button
              class="inline-flex items-center gap-1 text-2xs text-muted-foreground hover:text-foreground transition-colors cursor-pointer disabled:opacity-50"
              :disabled="modelCatalog.codexLoading"
              :title="t('settings.ai.reloadCodexModelsTitle')"
              @click="modelCatalog.refreshCodex()"
            >
              <component :is="modelCatalog.codexLoading ? Loader2 : RefreshCw" class="h-3 w-3" :class="{ 'animate-spin': modelCatalog.codexLoading }" :stroke-width="1.75" />
              {{ modelCatalog.codexLoading ? t('settings.ai.loading') : t('settings.ai.reload') }}
            </button>
          </div>
          <AppSelect size="sm" v-model="settings.codex_model" :options="codexModelOptions" />
          <p v-if="modelCatalog.codexError" class="text-2xs text-warning">{{ t('settings.ai.modelsRefreshError') }}</p>
          <p v-else-if="modelCatalog.codexRefreshedAt" class="text-2xs text-muted-foreground">{{ t('settings.ai.modelsCached', { count: modelCatalog.codexModels.length, when: fmtRefreshed(modelCatalog.codexRefreshedAt) }) }}</p>
          <p v-else class="text-2xs text-muted-foreground">{{ t('settings.ai.codexRefreshHint') }}</p>
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
      </div>

      <div class="h-px bg-border" />

      <!-- Group 2: translation of highlighted text -->
      <div class="space-y-4">
        <div class="flex items-center gap-1.5 text-muted-foreground">
          <Sparkles class="h-3 w-3" :stroke-width="1.75" />
          <span class="text-2xs font-semibold uppercase tracking-wider">{{ t('settings.ai.translation') }}</span>
        </div>
        <p class="text-2xs text-muted-foreground leading-relaxed">
          {{ t('settings.ai.translationHint') }}
        </p>
        <div class="space-y-1.5">
          <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.translateEngine') }}</label>
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
          <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.translateModel') }}</label>
          <AppSelect size="sm" v-model="settings.translate_model" :options="translateModelOptions" />
          <p class="text-2xs text-muted-foreground">{{ t('settings.ai.translateModelHint') }}</p>
        </div>
        <div class="space-y-1.5">
          <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.targetLanguage') }}</label>
          <AppSelect size="sm" v-model="settings.translate_target_lang" :options="TRANSLATE_TARGET_OPTIONS" />
        </div>
        <div class="space-y-1.5">
          <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.ai.translationStyle') }}</label>
          <AppSelect size="sm" v-model="settings.translate_style" :options="TRANSLATE_STYLE_OPTIONS" />
        </div>
      </div>
  </Card>
</template>
