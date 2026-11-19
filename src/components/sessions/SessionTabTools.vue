<script setup lang="ts">
// The Conversation's tools, at the right end of the tab bar (UR-07): the fork
// source link, show / fold the steps, the issue / PR Content column and the
// changed-files count. Icon-only with a tooltip, so the bar stays one 36px row.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { ChevronsDownUp, ChevronsUpDown, FileText, GitFork } from 'lucide-vue-next'
import MentionedFiles from '@/components/MentionedFiles.vue'
import { forkSource } from '@/lib/runFork'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const {
  currentRun, runsStore, runLabel, loadRunLog, isSessionWindow, uiLayout, liveHasStream, historyHasStream,
  isViewingHistory, canToggleContent, contentVisible, showContent, mentionedFileEntries,
} = useRunSession()

// A fork names the session it came from, and opens it (not from a pop-out,
// which stays on its one session).
const forkedFrom = computed(() => forkSource(currentRun.value, runsStore.runs))
const forkSourceRun = computed(() => (forkedFrom.value && forkedFrom.value !== 'gone' ? forkedFrom.value : null))
const forkLinkable = computed(() => !!forkSourceRun.value && !isSessionWindow)
const forkTip = computed(() => {
  if (!forkSourceRun.value) return `${t('run.fork.fromGone')}. ${t('run.fork.fromGoneTitle')}`
  return `${t('run.fork.from', { title: runLabel(forkSourceRun.value) })}. ${t('run.fork.fromTitle')}`
})
const hasSteps = computed(() => (isViewingHistory.value ? historyHasStream.value : liveHasStream.value))

function openForkSource() {
  if (forkLinkable.value && forkSourceRun.value) void loadRunLog(forkSourceRun.value.id)
}
</script>

<template>
  <div class="flex shrink-0 items-center gap-0.5 px-1">
    <button
      v-if="forkedFrom"
      type="button"
      class="tool-btn"
      :disabled="!forkLinkable"
      :title="forkTip"
      :aria-label="forkTip"
      @click="openForkSource"
    >
      <GitFork class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
    </button>
    <!-- Show the steps of every turn, or fold finished turns (the default).
         Remembered for this viewer. -->
    <button
      v-if="hasSteps"
      type="button"
      :class="['tool-btn', { 'is-on': uiLayout.stepsExpanded }]"
      :title="uiLayout.stepsExpanded ? t('streamLog.activity.collapseAll') : t('streamLog.activity.expandAll')"
      :aria-label="uiLayout.stepsExpanded ? t('streamLog.activity.collapseAll') : t('streamLog.activity.expandAll')"
      :aria-pressed="uiLayout.stepsExpanded"
      @click="uiLayout.setStepsExpanded(!uiLayout.stepsExpanded)"
    >
      <component :is="uiLayout.stepsExpanded ? ChevronsDownUp : ChevronsUpDown" class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
    </button>
    <!-- Issue / PR runs only: opens the fetched Content as a left split
         column. Sessions have no Content to show. -->
    <button
      v-if="canToggleContent"
      type="button"
      :class="['tool-btn', { 'is-on': contentVisible }]"
      :title="t('run.toggleContentPanel')"
      :aria-label="t('run.toggleContentPanel')"
      :aria-pressed="contentVisible"
      @click="showContent = !showContent"
    >
      <FileText class="h-4 w-4" :stroke-width="1.75" aria-hidden="true" />
    </button>
    <MentionedFiles :entries="mentionedFileEntries" />
  </div>
</template>

<style scoped>
.tool-btn {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border-radius: 6px;
  color: hsl(var(--muted-foreground));
  cursor: pointer;
}
.tool-btn:hover:not(:disabled) {
  background: hsl(var(--accent));
  color: hsl(var(--foreground));
}
.tool-btn:focus-visible {
  outline: 2px solid hsl(var(--ring));
  outline-offset: 1px;
}
.tool-btn:disabled {
  cursor: default;
}
.tool-btn.is-on {
  background: hsl(var(--primary) / 0.15);
  color: hsl(var(--primary));
}
</style>
