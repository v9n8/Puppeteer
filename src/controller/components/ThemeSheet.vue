<script setup lang="ts">
/**
 * Bottom sheet for this phone's look: light / dark / system and the color
 * palette, each either following the desktop or picked here. Each list shows
 * the value in force and ★ marks the desktop's. The choice is stored on this
 * device only (see ../appearance.ts) — nothing goes to the Host.
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Button } from '@/components/ui'
import { Check, Star } from 'lucide-vue-next'
import { COLOR_THEMES, NO_OVERRIDE, THEME_MODES, overrideForPick, resolveAppearance } from '@/lib/controllerAppearance'
import { colorThemeLabelKey } from '@/lib/colorThemes'
import { appearanceOverride, hostAppearance, setAppearanceOverride } from '../appearance'

const emit = defineEmits<{ close: [] }>()

const { t } = useI18n()

function modeLabel(mode: string): string {
  const key = { light: 'themeLight', dark: 'themeDark', system: 'themeSystem' }[mode]
  return key ? t(`settings.general.${key}`) : mode
}

function paletteLabel(palette: string): string {
  return t(colorThemeLabelKey(palette))
}

// The desktop's look (★; the built-in defaults until a Host is seen) and the one shown.
const desktop = computed(() => resolveAppearance(hostAppearance.value, NO_OVERRIDE))
const current = computed(() => resolveAppearance(hostAppearance.value, appearanceOverride.value))

const modeOptions = computed(() => THEME_MODES.map((m) => ({ value: m, label: modeLabel(m) })))
const paletteOptions = computed(() => COLOR_THEMES.map((p) => ({ value: p, label: paletteLabel(p) })))

function setMode(theme: string): void {
  setAppearanceOverride({ ...appearanceOverride.value, theme: overrideForPick(theme, desktop.value.theme) })
}

function setPalette(color_theme: string): void {
  setAppearanceOverride({ ...appearanceOverride.value, color_theme: overrideForPick(color_theme, desktop.value.color_theme) })
}
</script>

<template>
  <div class="absolute inset-0 z-20 flex flex-col justify-end">
    <div class="absolute inset-0 bg-black/50" @click="emit('close')" />
    <div
      class="relative flex max-h-[85%] flex-col rounded-t-2xl border-t border-border bg-card px-4 pt-2.5 pb-[max(1.25rem,env(safe-area-inset-bottom))] shadow-2xl"
    >
      <div class="mx-auto mb-3 h-1.5 w-10 shrink-0 rounded-full bg-foreground/20" />
      <p class="mb-1 text-sm font-semibold">{{ t('controller.theme.title') }}</p>
      <p class="mb-2 text-2xs text-muted-foreground">{{ t('controller.theme.hint') }}</p>

      <div class="min-h-0 overflow-auto">
        <h2 class="mt-2 mb-1.5 px-0.5 text-2xs font-medium uppercase tracking-wider text-muted-foreground">
          {{ t('controller.theme.mode') }}
        </h2>
        <div class="grid grid-cols-3 gap-1.5">
          <button
            v-for="o in modeOptions"
            :key="o.value"
            type="button"
            class="flex min-w-0 items-center justify-center gap-1 rounded-md border px-2 py-2 text-sm transition-colors cursor-pointer"
            :class="current.theme === o.value
              ? 'border-primary bg-primary/10 text-primary font-medium'
              : 'border-border bg-secondary text-foreground hover:bg-accent'"
            :aria-pressed="current.theme === o.value"
            @click="setMode(o.value)"
          >
            <Star
              v-if="o.value === desktop.theme"
              class="h-3 w-3 shrink-0 text-primary"
              :stroke-width="2"
              fill="currentColor"
              role="img"
              :aria-label="t('common.default')"
            />
            <span class="truncate">{{ o.label }}</span>
          </button>
        </div>

        <h2 class="mt-4 mb-1.5 px-0.5 text-2xs font-medium uppercase tracking-wider text-muted-foreground">
          {{ t('controller.theme.palette') }}
        </h2>
        <div class="overflow-hidden rounded-xl border border-border">
          <button
            v-for="o in paletteOptions"
            :key="o.value"
            type="button"
            class="flex w-full items-center gap-2.5 border-b border-border/60 px-3 py-2.5 text-left text-sm last:border-b-0 transition-colors cursor-pointer hover:bg-accent"
            :aria-pressed="current.color_theme === o.value"
            @click="setPalette(o.value)"
          >
            <span
              :data-theme="o.value === 'default' ? undefined : o.value"
              class="theme-scope h-4 w-4 shrink-0 rounded-full border border-black/10 bg-primary"
            />
            <span class="min-w-0 flex-1 truncate">{{ o.label }}</span>
            <Star
              v-if="o.value === desktop.color_theme"
              class="h-3.5 w-3.5 shrink-0 text-primary"
              :stroke-width="2"
              fill="currentColor"
              role="img"
              :aria-label="t('common.default')"
            />
            <Check
              v-if="current.color_theme === o.value"
              class="h-4 w-4 shrink-0 text-primary"
              :stroke-width="2.5"
            />
          </button>
        </div>
      </div>

      <Button size="md" variant="ghost" class="mt-3 shrink-0" @click="emit('close')">
        {{ t('controller.theme.close') }}
      </Button>
    </div>
  </div>
</template>
