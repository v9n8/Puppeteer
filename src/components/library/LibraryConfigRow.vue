<script setup lang="ts">
// A Hooks or Plugins row of the Library's list (LH): name, the command or
// description, then badges — a plugin's state, a hook file's disableAllHooks,
// the settings level and the engine.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertCircle } from 'lucide-vue-next'
import { Badge } from '@/components/ui'
import { pluginState, type ConfigItem } from '@/lib/libraryConfig'

const props = defineProps<{
  item: ConfigItem
  selected: boolean
}>()

const { t } = useI18n()

const state = computed(() => (props.item.plugin ? pluginState(props.item.plugin) : null))
</script>

<template>
  <span class="flex min-w-0 items-center gap-1.5">
    <span
      class="min-w-0 flex-1 truncate font-mono text-sm font-medium leading-snug"
      :class="selected && 'text-foreground'"
      :title="item.name"
    >
      {{ item.name }}
    </span>
    <span v-if="item.error" class="shrink-0 text-destructive" :title="item.error">
      <AlertCircle class="h-3.5 w-3.5" :stroke-width="1.75" />
    </span>
  </span>
  <span
    v-if="item.description"
    class="mt-1 truncate text-2xs text-muted-foreground/70"
    :class="item.hook && 'font-mono'"
    :title="item.description"
  >
    {{ item.description }}
  </span>
  <span class="mt-1 flex min-w-0 flex-wrap items-center gap-1">
    <Badge v-if="state" size="xs" :tone="state === 'on' ? 'success' : 'neutral'" class="shrink-0">
      {{ t(`library.config.state.${state}`) }}
    </Badge>
    <Badge
      v-if="item.plugin && !item.plugin.installed"
      size="xs"
      tone="warning"
      class="shrink-0"
      :title="t('library.config.notInstalledHint')"
    >
      {{ t('library.config.notInstalled') }}
    </Badge>
    <Badge v-if="item.hook?.all_disabled" size="xs" tone="warning" class="shrink-0" :title="t('library.config.allDisabled')">
      {{ t('library.config.hooksOff') }}
    </Badge>
    <Badge size="xs" class="shrink-0" :title="t(`library.config.sourceHint.${item.source}`)">
      {{ t(`library.config.source.${item.source}`) }}
    </Badge>
    <Badge size="xs" class="shrink-0">{{ t('library.engine.claude') }}</Badge>
  </span>
</template>
