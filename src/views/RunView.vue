<script setup lang="ts">
// The run screen: header, left rail, AI result pane, composer and conductor
// sidebar. Each part lives in components/sessions and shares this view's state
// through createRunSession / useRunSession (UX-03).
import RewindDialog from '@/components/sessions/RewindDialog.vue'
import RunHeader from '@/components/sessions/RunHeader.vue'
import RunSessionRail from '@/components/sessions/RunSessionRail.vue'
import RunLogPane from '@/components/sessions/RunLogPane.vue'
import RunComposer from '@/components/sessions/RunComposer.vue'
import RunWorkerSidebar from '@/components/sessions/RunWorkerSidebar.vue'
import RunEngineSwitchDialog from '@/components/sessions/RunEngineSwitchDialog.vue'
import { createRunSession } from '@/components/sessions/useRunSession'

const {
  uiLayout, isSessionWindow, composerVisible, showWorkerPanel, rewindTarget, onRewound,
} = createRunSession()
</script>

<template>
  <div class="flex flex-col h-full">
    <!-- Header -->
    <RunHeader />

    <div class="flex flex-1 overflow-hidden">
      <!-- Left panel: controls + history (hidden in focus mode, and in a session
           pop-out — that window is locked to its one run, no switcher). -->
      <RunSessionRail v-if="!uiLayout.focusMode && !isSessionWindow" />

      <!-- Terminal panel (split-pane: Content | AI Result) -->
      <div class="flex-1 flex flex-col overflow-hidden bg-background">
        <RunLogPane />

        <!-- Composer: prompt input on top, run controls in the footer below -->
        <RunComposer v-if="composerVisible" />
      </div>

      <!-- Conductor worker sidebar: for a live conductor, and also for a
           fresh session / issue / PR not yet launched (so its worker limit can be
           set before the goal is sent). The conductor itself is just this
           session; this panel adds its worker tree + control-plane timeline,
           collapsible to a thin rail. -->
      <RunWorkerSidebar v-if="showWorkerPanel" />
    </div>

    <RewindDialog
      :run-id="rewindTarget?.runId ?? null"
      :turn="rewindTarget?.turn ?? null"
      @close="rewindTarget = null"
      @done="onRewound"
    />

    <!-- Engine-switch dialog: new session vs. continue (carry context) -->
    <RunEngineSwitchDialog />
  </div>
</template>

<!-- Markdown output styling is global (src/assets/main.css): the streaming AI
     result is rendered via v-html inside <StreamLog>, which scoped :deep() rules
     here cannot reach. -->
