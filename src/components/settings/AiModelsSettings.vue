<script setup lang="ts">
// Settings → "Run defaults": default engine and permission mode. The default
// model of each engine sits on its card in "Engines & accounts"; translation has
// its own section (TranslateSettings).
import { useI18n } from 'vue-i18n'
import { ShieldAlert, Sparkles } from 'lucide-vue-next'
import { AppSelect, Card } from '@/components/ui'
import { useSettingsForm } from './useSettingsForm'

const { t } = useI18n()
const { settings } = useSettingsForm()
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Sparkles class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.ai.runDefaults') }}</span>
    </template>
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
  </Card>
</template>
