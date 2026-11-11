<script setup lang="ts">
// Left rail of the run screen: Session tab (new session, fetch issue/PR,
// History list) and Files tab. RunView hides it in focus mode and in a
// session pop-out. State lives in useRunSession.
import { useI18n } from 'vue-i18n'
import { runActivityAt } from '@/stores/runs'
import {
  GitPullRequest, Bug, Clock, MessageSquare, Trash2, X, ShieldQuestion, MessageCircleQuestion, Pin,
  Shield, Pencil, Check, FolderTree, MoreHorizontal, Search, Network, Plus, ArrowUpDown,
} from 'lucide-vue-next'
import RunActionItems from '@/components/RunActionItems.vue'
import FileTree from '@/components/FileTree.vue'
import { Button, Input, DropdownMenu } from '@/components/ui'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const {
  runsStore, project, currentRunId, runLabel, sessionListEl, loadRunLog, repos, refInput,
  detectedRef, fetching, fetchOpen, fetchPanelEl, fetchTriggerEl, historyDensity,
  leftTab, fetchError, needsLinkedIssue, linkedIssueInput, runIsRunning,
  runIcon, runStatusTitle, runWorkerCount, runRefIcon, runAccountModelLine, runRelativeTime,
  runDenseTooltip, runIconColorClass, runClaudeAccountLabel, runModelLabel, runStarted,
  runTimestamp, sessionSearch, filteredRuns, displayRuns, isProtectedByConductor, pendingRequest,
  handleFetch, handleSubmitLinkedIssue, clearingAll, actionsMenuRunId, renamingRunId, renameDraft,
  setRenameInputRef, cancelRename, commitRename, runActionProps, runActionHandlers,
  handleClearAllRuns, handleRefInput, creatingSession, handleNewSession, openFile,
} = useRunSession()
</script>

<template>
  <div class="w-72 shrink-0 border-r border-border/60 flex flex-col overflow-hidden bg-card/20">

    <!-- Rail tab switcher: Session (controls + history) vs Files (tree). -->
    <div class="flex shrink-0 border-b border-border/60 text-xs font-medium">
      <button
        class="flex-1 flex items-center justify-center gap-1.5 py-2 transition-colors cursor-pointer"
        :class="leftTab === 'session' ? 'text-foreground border-b-2 border-primary' : 'text-muted-foreground hover:text-foreground'"
        @click="leftTab = 'session'"
      >
        <MessageSquare class="h-3.5 w-3.5" :stroke-width="2" />
        {{ t('run.session') }}
      </button>
      <button
        class="flex-1 flex items-center justify-center gap-1.5 py-2 transition-colors cursor-pointer"
        :class="leftTab === 'files' ? 'text-foreground border-b-2 border-primary' : 'text-muted-foreground hover:text-foreground'"
        @click="leftTab = 'files'"
      >
        <FolderTree class="h-3.5 w-3.5" :stroke-width="2" />
        {{ t('run.files') }}
      </button>
    </div>

    <!-- Compact top toolbar: one New session button (every session is a
         conductor) and the Fetch issue/PR popover. Row density is set in
         Settings › General. -->
    <div v-show="leftTab === 'session'" class="p-3 border-b border-border/60">
      <div class="relative flex items-center gap-1.5">
        <Button class="flex-1" :disabled="creatingSession" @click="handleNewSession">
          <Plus class="h-3.5 w-3.5" :stroke-width="2" />
          {{ creatingSession ? t('run.creating') : t('run.newSession') }}
        </Button>
        <span ref="fetchTriggerEl" class="inline-flex">
          <Button
            variant="outline"
            size="icon"
            :class="{ 'bg-accent text-foreground': fetchOpen }"
            :title="t('run.fetchIssuePr')"
            :aria-label="t('run.fetchIssuePr')"
            :aria-expanded="fetchOpen"
            @click="fetchOpen = !fetchOpen"
          >
            <GitPullRequest class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
        </span>

      <!-- Fetch popover: spans the toolbar row (the rail clips overflow, so
           it can't hang off the icon itself). -->
      <div
        v-if="fetchOpen"
        ref="fetchPanelEl"
        class="absolute left-0 right-0 top-full z-30 mt-1.5 space-y-3 rounded-md border border-border bg-popover p-3 shadow-lg shadow-black/20"
      >
      <p class="text-2xs font-medium text-foreground/80">{{ t('run.fetchIssuePr') }}</p>

      <div v-if="repos.length === 0" class="text-2xs text-warning/80 leading-relaxed">
        {{ t('run.noReposConfigured') }}
      </div>

      <!-- The URL is the whole form: it names the repo, the kind and the
           number, so there is nothing left to pick by hand. -->
      <div>
        <Input
          :model-value="refInput"
          @update:model-value="handleRefInput"
          :placeholder="t('run.refPlaceholder')"
          @keyup.enter="handleFetch()"
        />
        <p v-if="fetchError" class="mt-1 text-2xs text-destructive">{{ fetchError }}</p>
      </div>

      <!-- Echo back what the URL was understood to mean. -->
      <div
        v-if="detectedRef"
        class="flex items-center gap-1.5 px-2 py-1 text-2xs rounded-md bg-muted text-muted-foreground"
      >
        <component
          :is="detectedRef.kind === 'pr' ? GitPullRequest : Bug"
          class="h-3 w-3 shrink-0"
          :stroke-width="1.75"
        />
        <span class="truncate">
          {{ detectedRef.kind === 'pr' ? t('run.labelPr') : t('run.issue') }}
          #{{ detectedRef.number }} · {{ detectedRef.repoName }}
        </span>
      </div>

      <Button
        v-if="!needsLinkedIssue"
        variant="outline"
        class="w-full"
        :disabled="fetching || !detectedRef"
        @click="handleFetch()"
      >
        {{ fetching
          ? t('run.fetching')
          : detectedRef?.kind === 'pr' ? t('run.fetchPr')
          : detectedRef?.kind === 'issue' ? t('run.fetchIssue')
          : t('run.fetchIssuePr') }}
      </Button>

      <!-- Linked issue prompt (shown when PR body has no Fixes/Closes/Resolves #N) -->
      <div v-if="needsLinkedIssue" class="space-y-2">
        <Input
          v-model="linkedIssueInput"
          :placeholder="t('run.linkedIssuePlaceholder')"
          @keyup.enter="handleSubmitLinkedIssue"
        />
        <Button
          variant="outline"
          class="w-full"
          :disabled="fetching || !linkedIssueInput"
          @click="handleSubmitLinkedIssue"
        >
          {{ fetching ? t('run.fetching') : t('run.fetchWithLinkedIssue') }}
        </Button>
      </div>
      </div>
      </div>
    </div>

    <!-- Run history -->
    <!-- The header (search + title/count + Clear all) is pinned: only the
         rows below it scroll, so the filter and the bulk action stay
         reachable no matter how far down the list the user is. -->
    <div v-show="leftTab === 'session'" class="flex-1 min-h-0 flex flex-col">
      <div class="shrink-0 border-b border-border/40">
        <div v-if="runsStore.runs.length > 0" class="px-3 pt-2.5">
          <div class="relative">
            <Search class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.75" />
            <Input
              v-model="sessionSearch"
              :placeholder="t('run.searchSessionsPlaceholder')"
              class="pl-8 pr-8"
            />
            <Button
              v-if="sessionSearch"
              variant="ghost"
              size="icon-xs"
              class="absolute right-1 top-1/2 -translate-y-1/2"
              :title="t('run.clearSearch')"
              :aria-label="t('run.clearSearch')"
              @click="sessionSearch = ''"
            >
              <X class="h-3.5 w-3.5" :stroke-width="2" />
            </Button>
          </div>
        </div>
        <div class="flex items-center justify-between px-4 py-1.5">
          <p class="text-2xs font-semibold text-muted-foreground uppercase tracking-wider">
            {{ t('run.history') }}<span v-if="displayRuns.length" class="font-normal"> · {{ displayRuns.length }}</span>
          </p>
          <button
            v-if="runsStore.orderStale"
            class="ml-auto mr-3 flex items-center gap-1 text-2xs text-primary/80 hover:text-primary transition-colors cursor-pointer"
            :title="t('run.resortSessionsTitle')"
            @click="runsStore.sortRuns()"
          >
            <ArrowUpDown class="h-2.5 w-2.5" :stroke-width="1.75" />
            {{ t('run.resortSessions') }}
          </button>
          <button
            v-if="runsStore.runs.length > 0"
            class="flex items-center gap-1 text-2xs text-muted-foreground/70 hover:text-destructive transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
            :disabled="clearingAll"
            :title="clearingAll ? t('run.clearing') : t('run.clearAllRunsTitle')"
            @click="handleClearAllRuns"
          >
            <Trash2 class="h-2.5 w-2.5" :stroke-width="1.75" />
            {{ clearingAll ? t('run.clearing') : t('run.clearAll') }}
          </button>
        </div>
      </div>
      <div ref="sessionListEl" class="flex-1 min-h-0 overflow-auto">
        <div v-if="runsStore.loading" class="px-4 py-3 text-xs text-muted-foreground">{{ t('common.loading') }}</div>
        <div v-else-if="runsStore.runs.length === 0" class="px-4 py-6 text-center text-xs text-muted-foreground">
          {{ t('run.noSessionsYet') }}
        </div>
        <div v-else-if="filteredRuns.length === 0" class="px-4 py-6 text-center text-xs text-muted-foreground">
          {{ t('run.noMatchingSessions') }}
        </div>
        <div
          v-else
          v-for="{ run } in displayRuns"
          :key="run.id"
          :data-run-id="run.id"
          class="group relative border-b border-border/30 transition-colors hover:bg-accent/40 focus-within:bg-accent/40"
          :class="{ 'bg-accent/60': currentRunId === run.id }"
        >
          <!-- Selected indicator bar -->
          <span
            v-if="currentRunId === run.id"
            class="absolute left-0 inset-y-0 w-0.5 bg-primary"
          />
          <!-- Row layout follows uiLayout.listDensity:
               comfortable = title line + badges/timestamp block,
               compact     = title line (+ relative time) + "account · model",
               dense       = one line; the rest lives in the row tooltip. -->
          <button
            class="w-full text-left px-3 cursor-pointer rounded-sm focus:outline-none focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-inset"
            :class="historyDensity === 'comfortable' ? 'py-3' : historyDensity === 'compact' ? 'py-2' : 'py-1.5'"
            :title="historyDensity === 'dense' ? runDenseTooltip(run) : undefined"
            @click="loadRunLog(run.id)"
          >
            <!-- Title line: status · badges · title · workers · markers · time -->
            <div class="flex items-center" :class="historyDensity === 'comfortable' ? 'gap-2' : 'gap-1.5'">
              <component
                :is="runIcon(run)"
                class="shrink-0"
                :class="[
                  historyDensity === 'comfortable' ? 'h-4 w-4' : 'h-3.5 w-3.5',
                  runIconColorClass(run),
                  { 'animate-spin': runIsRunning(run) },
                ]"
                :stroke-width="1.75"
                :title="runStatusTitle(run)"
              />
              <!-- Pin / protect badges grouped so they stay tightly aligned
                   when both are shown (own gap, not the row's gap). -->
              <span
                v-if="run.pinned || run.protected || isProtectedByConductor(run)"
                class="flex shrink-0 items-center gap-1"
              >
                <Pin
                  v-if="run.pinned"
                  class="h-3 w-3 text-primary"
                  :stroke-width="2"
                  :aria-label="t('run.pinned')"
                />
                <Shield
                  v-if="run.protected"
                  class="h-3 w-3 text-success"
                  :stroke-width="2"
                  :aria-label="t('run.protected')"
                />
                <Shield
                  v-else-if="isProtectedByConductor(run)"
                  class="h-3 w-3 text-success/50"
                  :stroke-width="2"
                  :aria-label="t('run.protectedByConductor')"
                  :title="t('run.protectedByConductor')"
                />
              </span>
              <!-- Issue / PR identity (the title already reads "Issue #N"). -->
              <component
                :is="runRefIcon(run)"
                v-if="runRefIcon(run)"
                class="h-3 w-3 shrink-0 text-muted-foreground"
                :stroke-width="1.75"
              />
              <span
                class="flex-1 min-w-0 truncate font-medium leading-tight"
                :class="historyDensity === 'dense' ? 'text-xs' : 'text-sm'"
                :title="historyDensity === 'dense' ? undefined : runLabel(run)"
              >{{ runLabel(run) }}</span>
              <!-- Worker count (its workers never get rows of their own). -->
              <span
                v-if="runWorkerCount(run) > 0"
                class="flex shrink-0 items-center gap-0.5 font-mono text-2xs text-muted-foreground"
                :title="t('conductor.workerCountShort', { n: runWorkerCount(run) })"
              >
                <Network class="h-3 w-3" :stroke-width="1.75" />{{ runWorkerCount(run) }}
              </span>
              <!-- Animated attention marker: this run is waiting for a
                   permission / question answer (replaces the floating toast). -->
              <span
                v-if="pendingRequest(run.id)"
                class="relative flex h-4 w-4 shrink-0 items-center justify-center text-primary"
                :title="
                  pendingRequest(run.id)?.tool_name === 'AskUserQuestion'
                    ? t('run.waitingForAnswer')
                    : t('run.waitingForPermission')
                "
              >
                <span class="absolute inset-0 animate-ping rounded-full bg-primary/30" />
                <component
                  :is="pendingRequest(run.id)?.tool_name === 'AskUserQuestion' ? MessageCircleQuestion : ShieldQuestion"
                  class="relative h-4 w-4"
                  :stroke-width="2"
                />
              </span>
              <!-- Relative age (compact/dense). In dense mode the ⋯ trigger
                   takes this slot on hover, so the time steps aside. -->
              <span
                v-if="historyDensity !== 'comfortable'"
                class="shrink-0 min-w-[1.25rem] text-right font-mono text-2xs text-muted-foreground/70"
                :class="historyDensity === 'dense'
                  ? ['group-hover:invisible group-focus-within:invisible', { invisible: actionsMenuRunId === run.id }]
                  : ''"
                :title="historyDensity === 'dense' ? undefined : runTimestamp(runActivityAt(run))"
              >{{ runRelativeTime(runActivityAt(run)) }}</span>
            </div>
            <!-- Compact: "account · model" (model without its "claude-"
                 prefix; full value in the tooltip). Kept in the layout but
                 invisible for an unstarted session so rows share one height.
                 `pr-6` keeps it clear of the hover actions (⋯). -->
            <p
              v-if="historyDensity === 'compact'"
              class="mt-0.5 pl-5 pr-6 truncate text-2xs leading-tight text-muted-foreground/70"
              :class="{ invisible: !runStarted(run) }"
              :title="runAccountModelLine(run, false)"
            >
              {{ runAccountModelLine(run) || '\u00a0' }}
            </p>
            <!-- Comfortable meta: the AI badges (account + model) on top so a
                 long model id gets the full row width, with the timestamp on
                 its own line underneath. -->
            <div
              v-else-if="historyDensity === 'comfortable'"
              class="mt-2 pl-6 pr-3 text-2xs text-muted-foreground/70"
              :class="{ invisible: !runStarted(run) }"
            >
              <!-- Account badge shown alongside the model when the run has a
                   known Claude account (the name implies Claude); Codex and
                   legacy/account-less runs show just the model. `pr-6` keeps
                   the badges clear of the hover actions (⋯) at the row's end. -->
              <div class="flex flex-wrap items-center gap-1.5 pr-6">
                <span
                  v-if="runClaudeAccountLabel(run)"
                  class="shrink-0 truncate max-w-[10rem] px-1.5 py-0.5 rounded bg-primary/10 text-2xs font-medium tracking-wide text-primary"
                  :title="runClaudeAccountLabel(run)"
                >
                  {{ runClaudeAccountLabel(run) }}
                </span>
                <!-- The model this run actually runs on (falls back to engine). -->
                <span
                  class="min-w-0 max-w-full truncate px-1.5 py-0.5 rounded bg-muted/60 font-mono text-2xs tracking-wide text-muted-foreground"
                  :title="runModelLabel(run)"
                >
                  {{ runModelLabel(run) }}
                </span>
              </div>
              <span class="mt-1 flex items-center gap-1" :title="t('run.lastActivity')">
                <Clock class="h-2.5 w-2.5 shrink-0" :stroke-width="1.5" />
                {{ runTimestamp(runActivityAt(run)) }}
              </span>
            </div>
          </button>
          <!-- Actions: a single overflow (⋯) menu revealed on hover/focus, so
               the row's meta info stays fully visible. The trigger also stays
               put while its menu is open (actionsMenuRunId). Running sessions
               get a safe subset — delete/refetch are hidden while live. -->
          <div
            class="absolute flex items-center opacity-0 pointer-events-none transition-opacity group-hover:opacity-100 group-hover:pointer-events-auto group-focus-within:opacity-100 group-focus-within:pointer-events-auto"
            :class="[
              historyDensity === 'comfortable' ? 'right-2 bottom-2'
              : historyDensity === 'compact' ? 'right-2 bottom-1'
              : 'right-2 top-1/2 -translate-y-1/2',
              { '!opacity-100 !pointer-events-auto': actionsMenuRunId === run.id },
            ]"
          >
            <DropdownMenu
              align="right"
              @update:open="(o) => (actionsMenuRunId = o ? run.id : null)"
            >
              <template #trigger>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  :aria-label="t('run.moreActions')"
                  :title="t('run.moreActions')"
                >
                  <MoreHorizontal class="h-3.5 w-3.5" :stroke-width="1.75" />
                </Button>
              </template>

              <RunActionItems v-bind="runActionProps(run)" section="actions" v-on="runActionHandlers(run)" />
              <RunActionItems v-bind="runActionProps(run)" section="delete" v-on="runActionHandlers(run)" />
            </DropdownMenu>
          </div>

          <!-- Inline rename: overlays the row title while editing. -->
          <div
            v-if="renamingRunId === run.id"
            class="absolute inset-0 z-20 flex items-center gap-2 px-3 bg-card"
            @click.stop
          >
            <Pencil class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="1.75" />
            <input
              :ref="setRenameInputRef"
              v-model="renameDraft"
              type="text"
              class="flex-1 min-w-0 bg-transparent text-sm font-medium leading-tight outline-none border-b border-primary/60 pb-0.5"
              :placeholder="t('run.runNamePlaceholder')"
              @keyup.enter="commitRename(run.id)"
              @keyup.esc="cancelRename()"
              @blur="commitRename(run.id)"
            />
            <Button variant="ghost" size="icon-sm" :aria-label="t('run.saveName')" :title="t('common.save')" @mousedown.prevent @click.stop="commitRename(run.id)">
              <Check class="h-3.5 w-3.5" :stroke-width="2" />
            </Button>
            <Button variant="ghost" size="icon-sm" :aria-label="t('run.cancelRename')" :title="t('common.cancel')" @mousedown.prevent @click.stop="cancelRename()">
              <X class="h-3.5 w-3.5" :stroke-width="2" />
            </Button>
          </div>
        </div>
      </div>
    </div>

    <!-- Files tab: VSCode-style lazy file tree for this project. -->
    <FileTree
      v-show="leftTab === 'files'"
      v-if="project?.path"
      :project-path="project.path"
      :active="leftTab === 'files'"
      @open-file="openFile"
    />

  </div>
</template>
