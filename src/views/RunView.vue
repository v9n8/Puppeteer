<script setup lang="ts">
// The run screen: left rail, then the session column (header, tab bar, AI result
// pane or a file tab, composer, info bar) and conductor sidebar. Each part lives in components/sessions and shares
// this view's state through createRunSession / useRunSession (UX-03).
import RewindDialog from '@/components/sessions/RewindDialog.vue'
import RunHeader from '@/components/sessions/RunHeader.vue'
import RunSessionRail from '@/components/sessions/RunSessionRail.vue'
import RunLogPane from '@/components/sessions/RunLogPane.vue'
import SessionTabBar from '@/components/sessions/SessionTabBar.vue'
import SessionSplitView from '@/components/sessions/SessionSplitView.vue'
import FileTabView from '@/components/sessions/FileTabView.vue'
import ChangedFilesTab from '@/components/sessions/ChangedFilesTab.vue'
import RunComposer from '@/components/sessions/RunComposer.vue'
import TurnStatusBar from '@/components/sessions/TurnStatusBar.vue'
import RunWorkerSidebar from '@/components/sessions/RunWorkerSidebar.vue'
import RunEngineSwitchDialog from '@/components/sessions/RunEngineSwitchDialog.vue'
import RunStatusBar from '@/components/sessions/RunStatusBar.vue'
import { computed } from 'vue'
import { createRunSession } from '@/components/sessions/useRunSession'
import { effectiveLayout, fileAreaTab, SPLIT_MIN_WINDOW } from '@/lib/sessionTabs'
import { useSessionTabsStore } from '@/stores/sessionTabs'

const {
  uiLayout, isSessionWindow, composerVisible, showWorkerPanel, rewindTarget, onRewound, closeRewind, currentRunId,
} = createRunSession()

// A file tab (V3-24) or the Changed files tab (V3-26) of this Session is selected:
// it takes the conversation's place; in the split view (V3-27) it is the right
// side's tab and the conversation stays on the left.
const sessionTabs = useSessionTabsStore()
const tabsState = computed(() => sessionTabs.stateFor(currentRunId.value ?? ''))
const layout = computed(() => effectiveLayout(tabsState.value, uiLayout.windowWidth))
const isSplit = computed(() => layout.value === 'split')
const narrowNotice = computed(() => !!tabsState.value.split?.on && tabsState.value.tabs.length > 0 && uiLayout.windowWidth < SPLIT_MIN_WINDOW)
const activeTab = computed(() => fileAreaTab(tabsState.value, layout.value))
const showingFile = computed(() => activeTab.value.startsWith('file:'))
const showingChanged = computed(() => activeTab.value === 'changed')

function onSplitCommit(leftPx: number, totalPx: number) {
  if (currentRunId.value) sessionTabs.setRatio(currentRunId.value, leftPx, totalPx)
}
function onSplitReset() {
  if (currentRunId.value) sessionTabs.resetRatio(currentRunId.value)
}
// Mounted while the tab exists, so its count stays right and it follows the Session.
const hasChangedTab = computed(() => !!currentRunId.value && sessionTabs.stateFor(currentRunId.value).tabs.some((t) => t.id === 'changed'))
</script>

<template>
  <div class="flex flex-col h-full">
    <div class="flex flex-1 overflow-hidden">
      <!-- Left panel: controls + history (hidden in focus mode, and in a session
           pop-out — that window is locked to its one run, no switcher). -->
      <RunSessionRail v-if="!uiLayout.focusMode && !isSessionWindow" />

      <!-- Terminal panel (split-pane: Content | AI Result) -->
      <div class="flex-1 flex flex-col overflow-hidden bg-background">
        <!-- Header: this session's name, bookmarks, find; as wide as the session column. -->
        <RunHeader />
        <!-- One frame for both layouts (V3-27): the parts only change grid cell, so the
             conversation keeps its scroll and its stream when the Session is split or not,
             or a file is read (hidden, never unmounted). -->
        <SessionSplitView
          :split="isSplit"
          :ratio="tabsState.split?.ratio ?? 0.5"
          :narrow-notice="narrowNotice"
          :show-log="!showingFile && !showingChanged"
          :show-file="showingFile || showingChanged"
          @commit="onSplitCommit"
          @reset="onSplitReset"
        >
          <template #tabbar><SessionTabBar v-if="currentRunId" :group="isSplit ? 'left' : 'all'" /></template>
          <template #rtabbar><SessionTabBar v-if="currentRunId && isSplit" group="right" /></template>
          <template #log><RunLogPane /></template>
          <template #file>
            <FileTabView v-if="currentRunId" v-show="showingFile" :active-id="activeTab" />
            <ChangedFilesTab v-if="hasChangedTab" v-show="showingChanged" />
          </template>
          <!-- Turn status bar (V3-17): what the last turn is doing; opens its steps and todo. -->
          <template #status><TurnStatusBar v-if="composerVisible" /></template>
          <!-- Composer: prompt input on top, run controls in the footer below -->
          <template #composer><RunComposer v-if="composerVisible" /></template>
          <!-- Info bar: usage, context, connections, links. Shown with the composer. -->
          <template #info><RunStatusBar v-if="composerVisible" /></template>
        </SessionSplitView>
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
      :edit-resend="!!rewindTarget?.editResend"
      @close="closeRewind"
      @done="(outcome) => onRewound(outcome)"
    />

    <!-- Engine-switch dialog: new session vs. continue (carry context) -->
    <RunEngineSwitchDialog />
  </div>
</template>

<!-- Markdown output styling is global (src/assets/main.css): the streaming AI
     result is rendered via v-html inside <StreamLog>, which scoped :deep() rules
     here cannot reach. -->
