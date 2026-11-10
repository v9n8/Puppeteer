<script setup lang="ts">
// Engine-switch dialog: start a new session, or continue this one on the other
// engine (hand-off). State lives in useRunSession.
import { useI18n } from 'vue-i18n'
import { Cpu, RotateCcw } from 'lucide-vue-next'
import { Button, Modal } from '@/components/ui'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const {
  features, loadedRunEngine, handoffTarget, handingOff, startingEngineSession, engineSwitchBusy,
  closeHandoffDialog, doHandoff, startNewEngineSession,
} = useRunSession()
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
        {{ t('run.switchTo') }} <span class="font-mono">{{ handoffTarget }}</span>
      </h3>
    </template>

    <div class="px-4 py-3 text-xs text-foreground/80 space-y-1.5">
      <p>
        {{ t('run.handoffIntro') }}
        <span class="font-mono">{{ loadedRunEngine || t('run.currentEngine') }}</span>.
        {{ t('run.handoffQuestion') }}
      </p>
      <ul class="list-disc pl-4 text-foreground/60 space-y-0.5">
        <li v-if="features.allows('run.handoff')">{{ t('run.handoffContinueBullet', { engine: handoffTarget }) }}</li>
        <li>{{ t('run.handoffNewBullet') }}</li>
      </ul>
    </div>

    <template #footer>
      <Button variant="outline" :disabled="engineSwitchBusy" @click="startNewEngineSession(handoffTarget!)">
        <RotateCcw v-if="startingEngineSession" class="h-3.5 w-3.5 animate-spin" :stroke-width="2" />
        {{ startingEngineSession ? t('run.creating') : t('run.startNewSession') }}
      </Button>
      <!-- Carrying the context over is the hand-off (`sessions.handoff`). -->
      <Button v-if="features.allows('run.handoff')" :disabled="engineSwitchBusy" @click="doHandoff(handoffTarget!)">
        <RotateCcw v-if="handingOff" class="h-3.5 w-3.5 animate-spin" :stroke-width="2" />
        {{ handingOff ? t('run.loadingContext') : t('run.continueCurrentSession') }}
      </Button>
    </template>
  </Modal>
</template>
