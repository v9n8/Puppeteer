<script setup lang="ts">
// The run screen's header: project name, the account / server / cloud chips,
// the linked-item menus and the ⋯ menu. State lives in useRunSession.
import { useI18n } from 'vue-i18n'
import {
  Terminal, Settings, Code2, FolderOpen, Maximize2, Minimize2, Check, HardDrive, Cloud, CloudCog,
  MoreHorizontal,
} from 'lucide-vue-next'
import RunActionItems from '@/components/RunActionItems.vue'
import RunLinkedKindMenu from '@/components/sessions/RunLinkedKindMenu.vue'
import { Button, Badge, DropdownMenu, DropdownItem, DropdownSeparator } from '@/components/ui'
import AddTodoButton from '@/components/focus/AddTodoButton.vue'
import { useRunSession } from './useRunSession'
import { projectPageQuery } from '@/lib/projectSections'

const { t } = useI18n()
const {
  router, features, uiLayout, projectId, project, isSessionWindow, currentRun, activeAccounts,
  visibleServerChips, hiddenServerChips, hiddenServerTitle, awsChip, azureChip, openQuickCapture,
  linked, handleOpenInVscode, handleOpenInFolder, handleOpenInTerminal, runGithubUrl,
  renameViaPrompt, runActionProps, runActionHandlers,
} = useRunSession()
</script>

<template>
  <div class="flex items-center justify-between gap-3 px-6 h-13 border-b border-border/60 shrink-0">
    <div class="flex items-center gap-2 min-w-0">
      <h1 class="text-sm font-semibold truncate">{{ project?.name ?? t('run.projectFallback') }}</h1>
      <Badge
        v-for="acc in activeAccounts"
        :key="acc.key"
        :tone="acc.tone"
        size="sm"
        class="font-mono shrink-0 inline-flex items-center gap-1"
        :title="acc.title"
      >
        <component :is="acc.icon" class="h-3 w-3" :stroke-width="1.75" />
        {{ acc.username }}
        <span v-if="acc.tag" class="font-sans text-2xs uppercase tracking-wide opacity-70">{{ acc.tag }}</span>
      </Badge>
      <!-- VPS servers wired to this project (transparent SSH per run). -->
      <Badge
        v-for="chip in visibleServerChips"
        :key="chip.key"
        :tone="chip.tone"
        size="sm"
        class="shrink-0 inline-flex items-center gap-1"
        :title="chip.title"
      >
        <HardDrive class="h-3 w-3" :stroke-width="2" />
        {{ chip.label }}
      </Badge>
      <Badge
        v-if="hiddenServerChips.length"
        tone="neutral"
        size="sm"
        class="shrink-0 inline-flex items-center gap-1"
        :title="hiddenServerTitle"
      >
        <HardDrive class="h-3 w-3" :stroke-width="2" />
        +{{ hiddenServerChips.length }}
      </Badge>
      <!-- Default AWS account (credentials brokered per run); +N other enabled. -->
      <Badge
        v-if="awsChip"
        tone="primary"
        size="sm"
        class="shrink-0 inline-flex items-center gap-1"
        :title="awsChip.title"
      >
        <Cloud class="h-3 w-3" :stroke-width="2" />
        {{ awsChip.label }}
        <span class="opacity-90 font-mono">{{ awsChip.region }}</span>
        <span v-if="awsChip.tag" class="font-sans text-2xs uppercase tracking-wide opacity-70">{{ awsChip.tag }}</span>
      </Badge>
      <!-- Default Azure account (az login isolated per account); +N other enabled. -->
      <Badge
        v-if="azureChip"
        tone="primary"
        size="sm"
        class="shrink-0 inline-flex items-center gap-1"
        :title="azureChip.title"
      >
        <CloudCog class="h-3 w-3" :stroke-width="2" />
        {{ azureChip.label }}
        <span v-if="azureChip.subscription" class="opacity-90">{{ azureChip.subscription }}</span>
        <span v-if="azureChip.tag" class="font-sans text-2xs uppercase tracking-wide opacity-70">{{ azureChip.tag }}</span>
      </Badge>
    </div>
    <!-- Icon-only actions: the linked Slack / todo / note menus, then one ⋯
         menu for the rest. Labels live in the tooltips so the header stays
         short no matter how narrow the pane gets.
         Hidden entirely in a session pop-out for now — that window is a bare
         conversation view. -->
    <div v-if="!isSessionWindow" class="flex items-center gap-1 shrink-0">
      <!-- What this session is linked to: Slack threads (only when there are
           some), todos, notes. The todo / note menus end with "+ New …", the
           capture filed under this run. -->
      <RunLinkedKindMenu v-if="features.allows('run.linkedSlack')" kind="slack" :threads="linked.threads.value" />
      <RunLinkedKindMenu
        v-if="features.allows('run.linkedTodos')"
        kind="todo"
        :items="linked.todos.value"
        @create="openQuickCapture('todo')"
      />
      <RunLinkedKindMenu
        v-if="features.allows('run.linkedNotes')"
        kind="note"
        :items="linked.notes.value"
        @create="openQuickCapture('note')"
      />

      <!-- A run made from an issue: "+ Todo" for that issue, linked to this run (Focus §3.1). -->
      <AddTodoButton
        v-if="currentRun?.run_type === 'analyze_issue' && runGithubUrl(currentRun)"
        :prefill="{
          text: currentRun.title ?? `#${currentRun.ref_number}`,
          projectId: currentRun.project_id,
          runId: currentRun.id,
          sourceUrl: runGithubUrl(currentRun),
        }"
        variant="icon"
      />

      <!-- Everything else lives in one ⋯ menu: open the project elsewhere ·
           this session's actions (the History row's, RunActionItems) ·
           remote / focus mode / project settings · delete. -->
      <DropdownMenu align="right">
        <template #trigger>
          <Button
            variant="outline"
            size="icon"
            class="relative"
            :title="t('run.moreActions')"
            :aria-label="t('run.moreActions')"
          >
            <MoreHorizontal class="h-3.5 w-3.5" :stroke-width="1.75" />
          </Button>
        </template>
        <div class="min-w-[15rem]">
          <!-- Open the project elsewhere -->
          <div class="px-2.5 pb-0.5 pt-1 text-2xs font-medium uppercase tracking-wider text-muted-foreground/70">
            {{ t('run.openProjectIn') }}
          </div>
          <DropdownItem :disabled="!project" @click="handleOpenInVscode">
            <Code2 class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
            VS Code
          </DropdownItem>
          <DropdownItem :disabled="!project" @click="handleOpenInFolder">
            <FolderOpen class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
            {{ t('run.folder') }}
          </DropdownItem>
          <DropdownItem :disabled="!project" @click="handleOpenInTerminal">
            <Terminal class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
            {{ t('run.terminal') }}
          </DropdownItem>

          <!-- This session (same list as its History row) -->
          <template v-if="currentRun">
            <DropdownSeparator />
            <RunActionItems
              v-bind="runActionProps(currentRun)"
              section="actions"
              v-on="runActionHandlers(currentRun, renameViaPrompt)"
            />
          </template>

          <DropdownSeparator />
          <DropdownItem
            :title="uiLayout.focusMode ? t('run.exitFocusMode') : t('run.enterFocusMode')"
            @click="uiLayout.toggleFocus()"
          >
            <component :is="uiLayout.focusMode ? Minimize2 : Maximize2" class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
            <span class="flex-1">{{ t('run.focusMode') }}</span>
            <Check v-if="uiLayout.focusMode" class="h-3.5 w-3.5 shrink-0 text-primary" :stroke-width="2" />
          </DropdownItem>
          <DropdownItem :title="t('run.projectSettingsTitle')" @click="router.push({ path: '/projects', query: projectPageQuery(projectId) })">
            <Settings class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
            {{ t('run.projectSettings') }}
          </DropdownItem>

          <RunActionItems
            v-if="currentRun"
            v-bind="runActionProps(currentRun)"
            section="delete"
            v-on="runActionHandlers(currentRun)"
          />
        </div>
      </DropdownMenu>
    </div>
  </div>
</template>
