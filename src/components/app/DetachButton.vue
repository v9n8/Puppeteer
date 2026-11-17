<script setup lang="ts">
// "Open in new window" in the header of a detachable screen (main window): the
// screen moves to its own window and stays there next time (lib/detachedScreens).
// Below 900 px only the icon is left; the tooltip still names the screen.
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { SquareArrowOutUpRight } from 'lucide-vue-next'
import { Button } from '@/components/ui'
import { detachScreen, type ScreenTarget } from '@/lib/detachedScreens'
import type { DetachableScreen } from '@/lib/windowPlacement'

const props = defineProps<{
  target: ScreenTarget
  /** Asked before moving; false keeps the screen here (e.g. unsaved edits the user keeps). */
  confirmLeave?: () => Promise<boolean>
  /** Set while moving is not possible (e.g. a copy under way): the button is locked and this is its tooltip. */
  lockedTip?: string
}>()

const NAME_KEYS: Record<DetachableScreen, string> = {
  tasks: 'nav.screenNames.tasks',
  notes: 'nav.screenNames.notes',
  slack: 'nav.screenNames.slack',
  calendar: 'nav.screenNames.calendar',
  gantt: 'nav.screenNames.gantt',
  'pr-inbox': 'nav.screenNames.prInbox',
  'secret-notes': 'nav.screenNames.secretNotes',
  settings: 'nav.screenNames.settings',
  'file-transfer': 'nav.screenNames.fileTransfer',
}

const router = useRouter()
const { t } = useI18n()
const busy = ref(false)
const tip = computed(
  () => props.lockedTip || t('nav.detachWindowTip', { screen: t(NAME_KEYS[props.target.screen]) }),
)

async function detach() {
  if (busy.value) return
  busy.value = true
  try {
    if (props.confirmLeave && !(await props.confirmLeave())) return
    await detachScreen(props.target, router)
  } catch (e) {
    console.error(`[DetachButton] cannot move ${props.target.screen} to its own window`, e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <Button variant="ghost" size="sm" :title="tip" :aria-label="tip" :disabled="busy || !!lockedTip" @click="detach">
    <SquareArrowOutUpRight class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
    <span class="max-[901px]:hidden">{{ t('nav.detachWindow') }}</span>
  </Button>
</template>
