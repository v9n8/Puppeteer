<script setup lang="ts">
// The title bar's command box: a visible way into the command palette (⌘K), for
// people who do not know the key. Shows the palette's current shortcut, so it
// follows Settings → Keyboard shortcuts. Hidden while the palette is off.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Search } from 'lucide-vue-next'
import { IS_MAC, effectiveKeymap, runMenuAction } from '@/lib/appMenu'
import { PALETTE_FEATURE } from '@/lib/commandPalette'
import { PALETTE_ACTION, formatAccelerator } from '@/lib/keymap'
import { useFeaturesStore } from '@/stores/features'

const { t } = useI18n()
const features = useFeaturesStore()

const shortcut = computed(() => {
  // effectiveKeymap is reactive; menuAccelerator is not, so a changed key would not show.
  const accel = effectiveKeymap().get(PALETTE_ACTION)
  return accel ? formatAccelerator(accel, IS_MAC) : ''
})
</script>

<template>
  <button
    v-if="features.isOn(PALETTE_FEATURE)"
    type="button"
    class="flex h-7 w-[180px] min-[1100px]:w-[280px] shrink-0 items-center gap-2 rounded-md border border-border/60 bg-background/60 px-2 text-left text-muted-foreground transition-colors hover:bg-accent/60 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    :aria-label="t('palette.box')"
    @click="runMenuAction(PALETTE_ACTION)"
  >
    <Search class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
    <span class="min-w-0 flex-1 truncate text-[13px]">{{ t('palette.box') }}</span>
    <kbd v-if="shortcut" class="shrink-0 rounded border border-border/70 px-1 font-sans text-2xs">{{ shortcut }}</kbd>
  </button>
</template>
