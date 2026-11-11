<script setup lang="ts">
// Settings → "Engines & accounts": the Claude and Codex accounts in one card,
// then one card per engine (CLI path, model list) and the extra CLI args under
// "Advanced". The default model of each engine is set in "Defaults for new
// sessions"; the cards link there.
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronRight, Cpu, Loader2, RefreshCw } from 'lucide-vue-next'
import { Card, InfoTip, Input } from '@/components/ui'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { useSettingsForm } from './useSettingsForm'
import { fmtRefreshed } from './useModelOptions'
import type { SettingsSectionId } from '@/lib/settingsSections'
import ClaudeEngineCard from './ClaudeEngineCard.vue'
import EngineAccountsCard from './EngineAccountsCard.vue'
import SettingsRow from './SettingsRow.vue'

const emit = defineEmits<{ openSection: [id: SettingsSectionId] }>()

const { t } = useI18n()
const modelCatalog = useModelCatalogStore()
const { settings } = useSettingsForm()

// The cached Codex list and the Claude validation report shown on the cards.
onMounted(() => {
  modelCatalog.ensureLoaded().catch(() => {})
})

// Collapsed by default, but open when extra args are set: they apply to every
// run, so they should not sit out of sight.
const advancedOpen = ref(!!settings.value.extra_args?.trim())
</script>

<template>
  <div class="space-y-4">
    <EngineAccountsCard />

    <ClaudeEngineCard @open-defaults="emit('openSection', 'ai')" />

    <Card body-class="p-4 space-y-4">
      <template #header>
        <Cpu class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('settings.engines.codex') }}</span>
      </template>
      <SettingsRow :label="t('settings.engine.codexPath')">
        <Input
          v-model="settings.codex_path"
          size="sm"
          placeholder="/usr/local/bin/codex"
          class="font-mono"
        />
      </SettingsRow>
      <div class="space-y-1.5">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-1.5">
            <span class="text-xs font-medium text-foreground">{{ t('settings.engines.models') }}</span>
            <InfoTip
              v-if="!modelCatalog.codexRefreshedAt && !modelCatalog.codexLoading && !modelCatalog.codexError"
              :text="t('settings.ai.codexRefreshHint')"
            />
          </div>
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
        <p v-if="modelCatalog.codexError" class="text-2xs text-warning">{{ t('settings.ai.modelsRefreshError') }}</p>
        <p v-else-if="modelCatalog.codexRefreshedAt" class="text-2xs text-muted-foreground">{{ t('settings.ai.modelsCached', { count: modelCatalog.codexModels.length, when: fmtRefreshed(modelCatalog.codexRefreshedAt) }) }}</p>
        <p class="text-2xs text-muted-foreground">
          {{ t('settings.engines.newSessionsUse', { model: settings.codex_model || t('settings.ai.modelDefault') }) }}
          <button type="button" class="text-primary hover:underline cursor-pointer" @click="emit('openSection', 'ai')">{{ t('settings.engines.change') }}</button>
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
