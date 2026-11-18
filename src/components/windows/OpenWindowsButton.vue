<script setup lang="ts">
// Title bar button of the "Open windows" overlay: the windows icon plus how many
// private windows are open (none: just the icon, it still opens the overlay).
// On while the overlay is showing.
import { computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { AppWindow } from 'lucide-vue-next'
import { IS_MAC, effectiveKeymap } from '@/lib/appMenu'
import { isRemoteMode } from '@/lib/ipc/remote'
import { OPEN_WINDOWS_ACTION, formatAccelerator } from '@/lib/keymap'
import { useOpenWindowsStore } from '@/stores/openWindows'

const { t } = useI18n()
const store = useOpenWindowsStore()
onMounted(() => store.start())

const tip = computed(() => {
  const accel = effectiveKeymap().get(OPEN_WINDOWS_ACTION)
  const keys = accel ? formatAccelerator(accel, IS_MAC) : ''
  return keys
    ? t('windows.buttonTip', { count: store.count, keys })
    : `${t('windows.title')} · ${store.count}`
})
</script>

<template>
  <button
    v-if="!isRemoteMode()"
    type="button"
    class="flex h-7 shrink-0 cursor-pointer items-center gap-1 rounded-sm px-1.5 text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    :class="store.open
      ? 'bg-accent text-foreground ring-1 ring-inset ring-input'
      : 'text-muted-foreground hover:bg-accent hover:text-foreground'"
    :title="tip"
    :aria-label="tip"
    :aria-pressed="store.open"
    @click="store.open = !store.open"
  >
    <AppWindow class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
    <span v-if="store.count > 0" class="tabular-nums">{{ store.count }}</span>
  </button>
</template>
