<script setup lang="ts">
// Settings → "Engines & accounts" → Claude card: CLI path and the model list's
// validation (the default model is set in "Defaults for new sessions"). The
// accounts moved to the shared Accounts card (EngineAccountsCard, UX-42); the
// path is a key of the shared settings form.
import { useI18n } from 'vue-i18n'
import { BadgeCheck, Cpu, Loader2 } from 'lucide-vue-next'
import { Card, InfoTip, Input } from '@/components/ui'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { useSettingsForm } from './useSettingsForm'
import { fmtRefreshed } from './useModelOptions'
import SettingsRow from './SettingsRow.vue'

const { t } = useI18n()
const { settings } = useSettingsForm()
const modelCatalog = useModelCatalogStore()
const emit = defineEmits<{ openDefaults: [] }>()
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Cpu class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.engines.claude') }}</span>
    </template>
    <SettingsRow :label="t('settings.engine.claudePath')">
      <Input
        v-model="settings.claude_path"
        size="sm"
        placeholder="/usr/local/bin/claude"
        class="font-mono"
      />
    </SettingsRow>
    <div class="space-y-1.5">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-1.5">
          <span class="text-xs font-medium text-foreground">{{ t('settings.engines.models') }}</span>
          <InfoTip
            v-if="!modelCatalog.validationCheckedAt && !modelCatalog.validating && !modelCatalog.validationError"
            :text="t('settings.ai.validateHint')"
          />
        </div>
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
      <p v-if="modelCatalog.validationError" class="text-2xs text-warning">{{ t('settings.ai.validateError') }}</p>
      <p v-else-if="modelCatalog.validating" class="text-2xs text-muted-foreground">{{ t('settings.ai.validateRunning') }}</p>
      <p v-else-if="modelCatalog.unsupportedModels.length" class="text-2xs text-warning">{{ t('settings.ai.validateSomeFailed', { models: modelCatalog.unsupportedModels.join(', '), when: fmtRefreshed(modelCatalog.validationCheckedAt) }) }}</p>
      <p v-else-if="modelCatalog.validationCheckedAt" class="text-2xs text-muted-foreground">{{ t('settings.ai.validateAllOk', { when: fmtRefreshed(modelCatalog.validationCheckedAt) }) }}</p>
      <p class="text-2xs text-muted-foreground">
        {{ t('settings.engines.newSessionsUse', { model: settings.claude_model || t('settings.ai.modelDefault') }) }}
        <button type="button" class="text-primary hover:underline cursor-pointer" @click="emit('openDefaults')">{{ t('settings.engines.change') }}</button>
      </p>
    </div>
  </Card>
</template>
