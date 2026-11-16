<script setup lang="ts">
// Settings → "Engines & accounts" → Claude card: CLI path, the default model
// (set in "Defaults for new sessions") and a link to Settings › Model, which
// scans the model list and picks the models offered. The accounts moved to the
// shared Accounts card (EngineAccountsCard, UX-42); the path is a key of the
// shared settings form.
import { useI18n } from 'vue-i18n'
import { Cpu } from 'lucide-vue-next'
import { Card, Input } from '@/components/ui'
import { useSettingsForm } from './useSettingsForm'
import SettingsRow from './SettingsRow.vue'

const { t } = useI18n()
const { settings } = useSettingsForm()
const emit = defineEmits<{ openDefaults: []; openModels: [] }>()
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
      <span class="text-xs font-medium text-foreground">{{ t('settings.engines.models') }}</span>
      <p class="text-2xs text-muted-foreground">
        {{ t('settings.engines.newSessionsUse', { model: settings.claude_model || t('settings.ai.modelDefault') }) }}
        <button type="button" class="text-primary hover:underline cursor-pointer" @click="emit('openDefaults')">{{ t('settings.engines.change') }}</button>
      </p>
      <p class="text-2xs text-muted-foreground">
        <button type="button" class="text-primary hover:underline cursor-pointer" @click="emit('openModels')">{{ t('settings.engines.manageModels') }}</button>
      </p>
    </div>
  </Card>
</template>
