<script setup lang="ts">
// Settings › General › Text: a corner of the session list (interface size) and
// one answer with code (conversation size, inside `.chat-text` like the real
// log), so a font, weight, smoothing or size change is seen before leaving.
import { useI18n } from 'vue-i18n'
import { Eye, Sparkles } from 'lucide-vue-next'

defineProps<{ summary: string }>()

const { t } = useI18n()
</script>

<template>
  <div class="rounded-md border border-border bg-background" aria-hidden="true">
    <div class="flex h-8 items-center gap-2 border-b border-border px-3 text-xs text-muted-foreground">
      <Eye class="h-4 w-4 shrink-0" :stroke-width="1.75" />
      <span>{{ t('settings.general.preview') }}</span>
      <span class="ml-auto truncate">{{ summary }}</span>
    </div>
    <div class="grid gap-4 p-4 min-[900px]:grid-cols-[13rem_1fr]">
      <div class="space-y-1 self-start rounded-md border border-border bg-sidebar p-1.5">
        <div class="relative rounded-md bg-primary/12 px-2.5 py-1.5 before:absolute before:inset-y-1.5 before:left-0 before:w-0.5 before:rounded-full before:bg-primary">
          <div class="truncate text-xs font-medium">{{ t('settings.general.previewRunTitle') }}</div>
          <div class="flex items-center gap-1.5 text-xs text-muted-foreground">
            <span class="h-2 w-2 rounded-full bg-info" />{{ t('settings.general.previewRunMeta') }}
          </div>
        </div>
        <div class="px-2.5 py-1.5">
          <div class="truncate text-xs font-medium">{{ t('settings.general.previewRun2Title') }}</div>
          <div class="flex items-center gap-1.5 text-xs text-muted-foreground">
            <span class="h-2 w-2 rounded-full bg-success" />{{ t('settings.general.previewRun2Meta') }}
          </div>
        </div>
      </div>
      <div class="chat-text min-w-0">
        <div class="flex items-center gap-2 text-xs">
          <span class="grid h-5 w-5 place-items-center rounded bg-primary text-primary-foreground">
            <Sparkles class="h-3 w-3" :stroke-width="2" />
          </span>
          <span class="font-medium">Claude</span>
        </div>
        <div class="markdown-output mt-1.5">
          <i18n-t keypath="settings.general.previewAnswer" tag="p">
            <template #file><code>RunLogPane.vue</code></template>
            <template #strong><strong>{{ t('settings.general.previewAnswerStrong') }}</strong></template>
          </i18n-t>
        </div>
        <div class="mt-2 flex h-8 items-center gap-2 rounded-md border border-border bg-sidebar px-3 font-mono text-xs">
          <span class="text-muted-foreground">$</span>
          <span class="truncate">pnpm vitest run RunLogPane</span>
        </div>
      </div>
    </div>
  </div>
</template>
