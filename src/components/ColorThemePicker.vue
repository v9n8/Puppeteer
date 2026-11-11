<script setup lang="ts">
/**
 * Color palette picker: a grid of preview tiles. Each preview is a
 * `.theme-scope` carrying its own `data-theme` (and `.dark` when the app is in
 * dark mode), so it paints that palette's tokens rather than the active one:
 * sidebar, text, primary button and the four status colors.
 */
import { onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Check } from 'lucide-vue-next'
import { COLOR_THEMES, colorThemeHintKey, colorThemeLabelKey } from '@/lib/colorThemes'

const model = defineModel<string>({ required: true })

const { t } = useI18n()

const STATUS_DOTS = ['bg-success', 'bg-warning', 'bg-destructive', 'bg-info']

const isDark = ref(document.documentElement.classList.contains('dark'))
let observer: MutationObserver | null = null
onMounted(() => {
  observer = new MutationObserver(() => {
    isDark.value = document.documentElement.classList.contains('dark')
  })
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })
})
onUnmounted(() => observer?.disconnect())
</script>

<template>
  <div class="grid grid-cols-2 gap-2 min-[1100px]:grid-cols-4">
    <button
      v-for="id in COLOR_THEMES"
      :key="id"
      type="button"
      class="flex flex-col gap-1.5 rounded-lg border bg-card p-1.5 text-left transition-colors cursor-pointer"
      :class="model === id ? 'border-primary ring-1 ring-primary' : 'border-border hover:border-input'"
      :aria-pressed="model === id"
      @click="model = id"
    >
      <span
        :data-theme="id === 'default' ? undefined : id"
        class="theme-scope flex h-16 overflow-hidden rounded-md border border-border bg-background text-foreground"
        :class="{ dark: isDark }"
        aria-hidden="true"
      >
        <span class="w-10 shrink-0 space-y-1 border-r border-border bg-sidebar p-1.5">
          <span class="block h-2 rounded-sm bg-primary" />
          <span class="block h-2 rounded-sm bg-muted-foreground" />
          <span class="block h-2 rounded-sm bg-border" />
        </span>
        <span class="flex min-w-0 flex-1 flex-col justify-between p-1.5">
          <span class="flex items-center justify-between gap-1">
            <span class="text-sm font-semibold leading-none">Aa</span>
            <span class="h-4 w-8 rounded bg-primary" />
          </span>
          <span class="flex gap-1">
            <span v-for="dot in STATUS_DOTS" :key="dot" class="h-2 w-2 rounded-full" :class="dot" />
          </span>
        </span>
      </span>
      <span class="flex items-center gap-1 px-1">
        <span class="min-w-0 flex-1 truncate text-xs font-medium">{{ t(colorThemeLabelKey(id)) }}</span>
        <Check v-if="model === id" class="h-4 w-4 shrink-0 text-primary" :stroke-width="2" />
      </span>
      <span class="px-1 pb-0.5 text-xs text-muted-foreground text-pretty">{{ t(colorThemeHintKey(id)) }}</span>
    </button>
  </div>
</template>
