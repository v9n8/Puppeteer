<script setup lang="ts">
// Settings → "Engine Paths": CLI paths of claude / codex and extra CLI args.
import { useI18n } from 'vue-i18n'
import { Cpu } from 'lucide-vue-next'
import { Card, Input } from '@/components/ui'
import { useSettingsForm } from './useSettingsForm'

const { t } = useI18n()
const { settings } = useSettingsForm()
</script>

<template>
  <Card body-class="p-4 space-y-4">
    <template #header>
      <Cpu class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
      <span class="text-xs font-semibold">{{ t('settings.engine.title') }}</span>
    </template>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.engine.claudePath') }}</label>
        <Input
          v-model="settings.claude_path"
          size="sm"
          placeholder="/usr/local/bin/claude"
          class="font-mono"
        />
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.engine.codexPath') }}</label>
        <Input
          v-model="settings.codex_path"
          size="sm"
          placeholder="/usr/local/bin/codex"
          class="font-mono"
        />
      </div>
      <div class="space-y-1.5">
        <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">
          {{ t('settings.engine.extraArgs') }}
          <span class="normal-case font-normal text-muted-foreground ml-1">{{ t('settings.engine.extraArgsHint') }}</span>
        </label>
        <Input
          v-model="settings.extra_args"
          size="sm"
          placeholder="--no-cache"
          class="font-mono"
        />
      </div>
  </Card>
</template>
