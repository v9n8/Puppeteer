<script setup lang="ts">
// Settings → "Engines & accounts": the Claude and Codex accounts in one card,
// then one card per engine (CLI path, default model) and the extra CLI args under
// "Advanced". The default model of each engine is set in "Defaults for new
// sessions" and the model lists in Settings › Model; the cards link there.
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronRight, Cpu } from 'lucide-vue-next'
import { Card, Input } from '@/components/ui'
import { useSettingsForm } from './useSettingsForm'
import type { SettingsSectionId } from '@/lib/settingsSections'
import ClaudeEngineCard from './ClaudeEngineCard.vue'
import EngineAccountsCard from './EngineAccountsCard.vue'
import SettingsRow from './SettingsRow.vue'
import SettingsSectionHead from './SettingsSectionHead.vue'

const emit = defineEmits<{ openSection: [id: SettingsSectionId] }>()

const { t } = useI18n()
const { settings } = useSettingsForm()

// Collapsed by default, but open when extra args are set: they apply to every
// run, so they should not sit out of sight.
const advancedOpen = ref(!!settings.value.extra_args?.trim())
</script>

<template>
  <div class="space-y-4">
    <SettingsSectionHead :title="t('settings.sections.engines')" :intro="t('settings.engines.intro')" />

    <EngineAccountsCard />

    <ClaudeEngineCard @open-defaults="emit('openSection', 'ai')" @open-models="emit('openSection', 'model')" />

    <Card body-class="p-4 space-y-4">
      <template #header>
        <Cpu class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('settings.engines.codex') }}</span>
      </template>
      <SettingsRow search-id="engines-codexPath" :label="t('settings.engine.codexPath')">
        <Input
          v-model="settings.codex_path"
          size="sm"
          placeholder="/usr/local/bin/codex"
          class="font-mono"
        />
      </SettingsRow>
      <div class="space-y-1.5">
        <span class="text-xs font-medium text-foreground">{{ t('settings.engines.models') }}</span>
        <p class="text-2xs text-muted-foreground">
          {{ t('settings.engines.newSessionsUse', { model: settings.codex_model || t('settings.ai.modelDefault') }) }}
          <button type="button" class="text-primary hover:underline cursor-pointer" @click="emit('openSection', 'ai')">{{ t('settings.engines.change') }}</button>
        </p>
        <p class="text-2xs text-muted-foreground">
          <button type="button" class="text-primary hover:underline cursor-pointer" @click="emit('openSection', 'model')">{{ t('settings.engines.manageModels') }}</button>
        </p>
      </div>
    </Card>

    <Card body-class="p-4 space-y-4">
      <button
        type="button"
        class="w-full flex items-center gap-2 text-left cursor-pointer"
        :aria-expanded="advancedOpen"
        @click="advancedOpen = !advancedOpen"
      >
        <ChevronRight
          class="h-3.5 w-3.5 text-muted-foreground transition-transform"
          :class="{ 'rotate-90': advancedOpen }"
          :stroke-width="1.75"
        />
        <span class="text-xs font-semibold">{{ t('settings.engines.advanced') }}</span>
      </button>
      <SettingsRow v-if="advancedOpen" :label="t('settings.engine.extraArgs')" :hint="t('settings.engine.extraArgsHint')">
        <Input
          v-model="settings.extra_args"
          size="sm"
          placeholder="--no-cache"
          class="font-mono"
        />
      </SettingsRow>
    </Card>
  </div>
</template>
