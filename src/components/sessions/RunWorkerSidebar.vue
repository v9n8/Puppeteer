<script setup lang="ts">
// Conductor worker sidebar (or its thin rail). RunView shows it for a live or
// pre-launch conductor. State lives in useRunSession.
import { useI18n } from 'vue-i18n'
import { Cpu, Pencil, Loader2, Network, ChevronLeft, ChevronRight } from 'lucide-vue-next'
import ConductorWorkerPanel from '@/components/conductor/ConductorWorkerPanel.vue'
import { Button, Input } from '@/components/ui'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const {
  conductorStore, currentRunId, conductorWorkers, conductorEvents, workerPanelOpen,
  setWorkerPanelOpen, editingWorkerCap, workerCapDraft, savingWorkerCap, beginEditWorkerCap,
  editWorkerCapFromRail, cancelEditWorkerCap, commitWorkerCap, pendingConductor,
  effectiveWorkerCap, openSession,
} = useRunSession()
</script>

<template>
  <aside
    class="shrink-0 border-l border-border/60 flex flex-col overflow-hidden bg-card/20 transition-[width]"
    :class="workerPanelOpen ? 'w-80' : 'w-9'"
  >
    <div class="flex items-center gap-1.5 px-2 h-8 border-b border-border/60 shrink-0">
      <Button
        variant="ghost"
        size="icon-xs"
        :title="workerPanelOpen ? t('conductor.collapse') : t('conductor.expand')"
        :aria-label="workerPanelOpen ? t('conductor.collapse') : t('conductor.expand')"
        @click="setWorkerPanelOpen(!workerPanelOpen)"
      >
        <component :is="workerPanelOpen ? ChevronRight : ChevronLeft" class="h-3.5 w-3.5" :stroke-width="2" />
      </Button>
      <template v-if="workerPanelOpen">
        <Network class="h-3.5 w-3.5 text-foreground/40 shrink-0" :stroke-width="1.75" />
        <span class="text-xs font-medium text-foreground/90 truncate">{{ t('conductor.workers') }}</span>
        <!-- Live count / editable cap. Click the cap to raise or lower the
             worker limit — before launch it seeds the launch cap, after it
             changes the running conductor's limit live. Rendered as a compact
             pill (running / limit) with the pencil revealed on hover. -->
        <div
          v-if="editingWorkerCap"
          class="ml-auto flex items-center gap-1 shrink-0 rounded-md border border-border bg-background pl-1.5 pr-1 h-6"
        >
          <Cpu class="h-3 w-3 text-muted-foreground shrink-0" :stroke-width="1.75" />
          <span class="text-2xs font-mono text-muted-foreground">{{ conductorWorkers.length }}/</span>
          <Input
            v-model="workerCapDraft"
            type="number"
            min="1"
            max="100"
            autofocus
            class="h-5 w-10 px-1 text-center text-2xs"
            @keyup.enter="commitWorkerCap"
            @keyup.esc="cancelEditWorkerCap"
            @blur="commitWorkerCap"
          />
        </div>
        <button
          v-else
          type="button"
          class="group/cap ml-auto flex items-center gap-1 shrink-0 rounded-md border border-transparent px-1.5 h-6 text-2xs font-mono text-muted-foreground hover:text-foreground hover:border-border hover:bg-accent/60 transition-colors cursor-pointer disabled:opacity-50"
          :title="t('conductor.editMaxWorkers')"
          :aria-label="t('conductor.editMaxWorkers')"
          :disabled="savingWorkerCap"
          @click="beginEditWorkerCap"
        >
          <Cpu class="h-3 w-3 shrink-0" :stroke-width="1.75" />
          <span :title="t('conductor.workerCount', { n: conductorWorkers.length, max: effectiveWorkerCap })">
            <span class="text-foreground/90">{{ conductorWorkers.length }}</span>/{{ effectiveWorkerCap }}
          </span>
          <component
            :is="savingWorkerCap ? Loader2 : Pencil"
            class="h-2.5 w-2.5 shrink-0 opacity-0 transition-opacity group-hover/cap:opacity-100"
            :class="{ 'animate-spin !opacity-100': savingWorkerCap }"
            :stroke-width="1.75"
          />
        </button>
      </template>
    </div>
    <!-- Rail: the worker cap stays reachable (notably before launch, when
         no worker exists yet) — clicking expands the panel into the editor. -->
    <button
      v-if="!workerPanelOpen"
      type="button"
      class="mx-auto mt-1.5 flex shrink-0 flex-col items-center gap-0.5 rounded-md px-1 py-1 font-mono text-2xs leading-none text-muted-foreground hover:text-foreground hover:bg-accent transition-colors cursor-pointer"
      :title="pendingConductor ? t('conductor.preLaunchHint') : t('conductor.editMaxWorkers')"
      :aria-label="t('conductor.editMaxWorkers')"
      @click="editWorkerCapFromRail"
    >
      <Cpu class="h-3 w-3" :stroke-width="1.75" />
      <span><span class="text-foreground/90">{{ conductorWorkers.length }}</span>/{{ effectiveWorkerCap }}</span>
    </button>
    <!-- Pre-launch: no workers/timeline yet — guide the user to set the limit
         and send the goal, rather than showing an empty worker tree. -->
    <div
      v-if="workerPanelOpen && pendingConductor"
      class="p-3 text-2xs leading-relaxed text-muted-foreground"
    >
      {{ t('conductor.preLaunchHint') }}
    </div>
    <ConductorWorkerPanel
      v-else
      :collapsed="!workerPanelOpen"
      :workers="conductorWorkers"
      :events="conductorEvents"
      :active-worker-id="currentRunId"
      :conductor-run-id="conductorStore.selectedId"
      @open-session="openSession"
      @back-to-conductor="openSession(conductorStore.selectedId ?? '')"
    />
  </aside>
</template>
