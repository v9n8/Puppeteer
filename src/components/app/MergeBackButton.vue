<script setup lang="ts">
// "Move back to main window" in the title bar of a screen's own window: the main
// window opens the screen and this window closes (lib/detachedScreens). Only for
// detachable screens — the permission prompt, Quick capture and Remote desktop
// always live in their own window. Below 900 px only the icon is left.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { SquareArrowDownLeft } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import { mergeScreen } from '@/lib/detachedScreens'
import type { DetachableScreen } from '@/lib/windowPlacement'

const props = defineProps<{
  screen: DetachableScreen
  path: string
  /** Asked before moving; false keeps this window open (e.g. unsaved edits the user keeps). */
  confirmLeave?: () => Promise<boolean>
  /** Tooltip when it is not "Move back to main window" (e.g. the screen goes to the remote-desktop window). */
  tip?: string
  /** Set while moving is not possible (e.g. a copy under way): the button is locked and this is its tooltip. */
  lockedTip?: string
}>()

const { t } = useI18n()
const busy = ref(false)
const tooltip = computed(() => props.lockedTip || props.tip || t('nav.mergeBack'))

async function merge() {
  if (busy.value) return
  busy.value = true
  try {
    if (props.confirmLeave && !(await props.confirmLeave())) return
    await mergeScreen({ screen: props.screen, path: props.path })
  } catch (e) {
    console.error(`[MergeBackButton] cannot move ${props.screen} back to the main window`, e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <Button
    variant="ghost"
    size="sm"
    :title="tooltip"
    :aria-label="tooltip"
    :disabled="busy || !!lockedTip"
    @click="merge"
  >
    <SquareArrowDownLeft class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
    <span class="max-[901px]:hidden">{{ t('nav.mergeBack') }}</span>
  </Button>
</template>
