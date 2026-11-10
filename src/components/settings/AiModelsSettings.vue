<script setup lang="ts">
// Settings → "AI & Models": run defaults (engine, models, permission mode).
// Translation has its own section (TranslateSettings).
import { useI18n } from 'vue-i18n'
import { BadgeCheck, Cpu, Loader2, RefreshCw, ShieldAlert, Sparkles } from 'lucide-vue-next'
import { AppSelect, Card } from '@/components/ui'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { useSettingsForm } from './useSettingsForm'
import { useModelOptions } from './useModelOptions'

const { t } = useI18n()
const modelCatalog = useModelCatalogStore()
const { settings } = useSettingsForm()
const { claudeModelOptions, codexModelOptions } = useModelOptions()

/** Format a cache timestamp for the "last refreshed" hint. */
function fmtRefreshed(iso: string | null | undefined): string {
  if (!iso) return ''
  const d = new Date(iso)
  return isNaN(d.getTime()) ? '' : d.toLocaleString()
}
</script>

<template>
  <Card body-class="p-4 space-y-5">
    <template #header>
      <Sparkles class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.ai.title') }}</span>
    </template>

      <!-- Run defaults -->
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
  </Card>
</template>
