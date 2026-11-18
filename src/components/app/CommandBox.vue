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
import { Kbd } from '@/components/ui'

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
    class="flex h-7 w-[180px] min-[1100px]:w-[280px] shrink-0 items-center gap-2 rounded-sm border border-input bg-card pl-2.5 pr-1.5 text-left text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    :aria-label="t('palette.box')"
    @click="runMenuAction(PALETTE_ACTION)"
  >
    <Search class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
    <span class="min-w-0 flex-1 truncate text-sm">{{ t('palette.box') }}</span>
    <Kbd v-if="shortcut" class="shrink-0">{{ shortcut }}</Kbd>
  </button>
</template>
