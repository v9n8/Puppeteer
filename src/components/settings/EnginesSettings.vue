<script setup lang="ts">
// Settings → "Engines & accounts": one card per engine (CLI path, model list,
// Claude's accounts) and the extra CLI args under "Advanced". The default model of
// each engine is set in "Defaults for new sessions"; the cards link there.
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronRight, Cpu, Loader2, RefreshCw } from 'lucide-vue-next'
import { Card, Input } from '@/components/ui'
import { useModelCatalogStore } from '@/stores/modelCatalog'
import { useSettingsForm } from './useSettingsForm'
import { fmtRefreshed } from './useModelOptions'
import type { SettingsSectionId } from '@/lib/settingsSections'
import ClaudeEngineCard from './ClaudeEngineCard.vue'

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
    <ClaudeEngineCard @open-defaults="emit('openSection', 'ai')" />

    <Card body-class="p-4 space-y-4">
      <template #header>
        <Cpu class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('settings.engines.codex') }}</span>
      </template>
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
        <div class="flex items-center justify-between">
          <label class="text-2xs font-medium text-muted-foreground uppercase tracking-wider">{{ t('settings.engines.models') }}</label>
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
        <p v-else class="text-2xs text-muted-foreground">{{ t('settings.ai.codexRefreshHint') }}</p>
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
      <div v-if="advancedOpen" class="space-y-1.5">
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
  </div>
</template>
