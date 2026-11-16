<script setup lang="ts">
// "Move back to main window" in the title bar of a screen's own window: the main
// window opens the screen and this window closes (lib/detachedScreens). Only for
// detachable screens — the permission prompt, Quick capture and Remote desktop
// always live in their own window. Below 900 px only the icon is left.
import { ref } from 'vue'
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
}>()

const { t } = useI18n()
const busy = ref(false)

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
    :title="t('nav.mergeBack')"
    :aria-label="t('nav.mergeBack')"
    :disabled="busy"
    @click="merge"
  >
    <SquareArrowDownLeft class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
    <span class="max-[900px]:hidden">{{ t('nav.mergeBack') }}</span>
  </Button>
</template>
