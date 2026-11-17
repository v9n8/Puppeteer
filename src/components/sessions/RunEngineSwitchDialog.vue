<script setup lang="ts">
// Engine-switch dialog: start a new session, or continue this one on the other
// engine (hand-off). State lives in useRunSession.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowRight, Cpu, Plus, RotateCcw } from 'lucide-vue-next'
import { Button, Modal } from '@/components/ui'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const {
  features, loadedRunEngine, handoffTarget, handingOff, startingEngineSession, engineSwitchBusy,
  closeHandoffDialog, doHandoff, startNewEngineSession,
} = useRunSession()

const capitalized = (name: string | null | undefined) => (name ? name.charAt(0).toUpperCase() + name.slice(1) : '')
const targetName = computed(() => capitalized(handoffTarget.value))
const fromName = computed(() => capitalized(loadedRunEngine.value) || t('run.currentEngine'))

// Tab already walks the two blocks; the arrows do too.
function onChoiceKeydown(event: KeyboardEvent) {
  const step = event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0
  if (!step) return
  const group = event.currentTarget as HTMLElement
  const items = Array.from(group.querySelectorAll<HTMLButtonElement>('[data-handoff-choice]:not(:disabled)'))
  if (!items.length) return
  event.preventDefault()
  const at = items.indexOf(document.activeElement as HTMLButtonElement)
  items[(at + step + items.length) % items.length].focus()
}
</script>

<template>
  <Modal
    :open="!!handoffTarget"
    size="sm"
    :closable="!engineSwitchBusy"
    @close="closeHandoffDialog"
  >
    <template #header>
      <Cpu class="h-4 w-4 text-primary shrink-0" :stroke-width="2" />
      <h3 class="text-sm font-semibold flex-1">
        {{ t('run.switchTo') }} <span class="font-mono">{{ targetName }}</span>
      </h3>
    </template>

    <div class="space-y-3 px-4 py-3 text-xs">
      <p class="text-foreground/80">
        {{ t('run.handoffIntro') }}
        <span class="font-mono">{{ fromName }}</span>.
        {{ t('run.handoffQuestion') }}
      </p>
      <div class="space-y-2" role="group" @keydown="onChoiceKeydown">
        <button
          type="button"
          data-handoff-choice
          class="flex w-full cursor-pointer items-start gap-3 rounded-md border border-border px-3 py-2.5 text-left hover:bg-muted focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-default disabled:opacity-60 disabled:hover:bg-transparent"
          :disabled="engineSwitchBusy"
          @click="startNewEngineSession(handoffTarget!)"
        >
          <RotateCcw v-if="startingEngineSession" class="mt-0.5 h-4 w-4 shrink-0 animate-spin text-primary" :stroke-width="2" />
          <Plus v-else class="mt-0.5 h-4 w-4 shrink-0 text-primary" :stroke-width="2" />
          <span class="flex min-w-0 flex-col gap-0.5">
            <span class="text-sm font-semibold">
              {{ startingEngineSession ? t('run.creating') : t('run.handoff.newTitle', { engine: targetName }) }}
            </span>
            <span class="text-foreground">{{ t('run.handoff.newBody') }}</span>
          </span>
        </button>
        <!-- Carrying the context over is the hand-off (`sessions.handoff`). -->
        <button
          v-if="features.allows('run.handoff')"
          type="button"
          data-handoff-choice
          class="flex w-full cursor-pointer items-start gap-3 rounded-md border border-border px-3 py-2.5 text-left hover:bg-muted focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-default disabled:opacity-60 disabled:hover:bg-transparent"
          :disabled="engineSwitchBusy"
          @click="doHandoff(handoffTarget!)"
        >
          <RotateCcw v-if="handingOff" class="mt-0.5 h-4 w-4 shrink-0 animate-spin text-primary" :stroke-width="2" />
          <ArrowRight v-else class="mt-0.5 h-4 w-4 shrink-0 text-primary" :stroke-width="2" />
          <span class="flex min-w-0 flex-col gap-0.5">
            <span class="text-sm font-semibold">
              {{ handingOff ? t('run.summarizingContext') : t('run.handoff.continueTitle') }}
            </span>
            <span class="text-foreground">{{ t('run.handoff.continueBody', { from: fromName, to: targetName }) }}</span>
          </span>
        </button>
      </div>
    </div>

    <template #footer>
      <Button variant="outline" :disabled="engineSwitchBusy" @click="closeHandoffDialog">
        {{ t('common.cancel') }}
      </Button>
    </template>
  </Modal>
</template>
