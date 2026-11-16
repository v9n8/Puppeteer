<script setup lang="ts">
// The run screen's header: the session's name (click to rename), the Bookmarks and Find buttons and the ⋯ menu.
// Connections and links live in the info bar under the composer. State lives in useRunSession.
import { useI18n } from 'vue-i18n'
import {
  Terminal, Settings, Code2, FolderOpen, Maximize2, Minimize2, Check, MoreHorizontal, Search,
} from 'lucide-vue-next'
import RunActionItems from '@/components/RunActionItems.vue'
import { Button, DropdownMenu, DropdownItem, DropdownSeparator } from '@/components/ui'
import SessionTitle from '@/components/sessions/SessionTitle.vue'
import SessionFlagChip from '@/components/sessions/SessionFlagChip.vue'
import SessionBookmarksMenu from '@/components/sessions/SessionBookmarksMenu.vue'
import { useRunSession } from './useRunSession'
import { projectPageQuery } from '@/lib/projectSections'

const { t } = useI18n()
const {
  router, uiLayout, projectId, project, isSessionWindow, currentRun, handleOpenInVscode,
  handleOpenInFolder, handleOpenInTerminal, renameViaPrompt, runActionProps, runActionHandlers, runFind,
} = useRunSession()
</script>

<template>
  <div class="flex items-center justify-between gap-3 px-4 h-12 border-b border-border/60 shrink-0">
    <div class="flex items-center gap-2 min-w-0">
      <SessionTitle />
      <SessionFlagChip />
    </div>
    <!-- Icon-only actions, then one ⋯ menu for the rest. Labels live in the tooltips so the header stays
         short no matter how narrow the pane gets.
         Bookmarks and Find stay in a session pop-out; the rest is hidden there
         for now (that window is a bare conversation view). -->
    <div class="flex items-center gap-1 shrink-0">
      <SessionBookmarksMenu />
      <!-- ⌘F opens the same bar (App.vue dispatches the key); this is its button. -->
      <Button
        v-if="runFind.enabled.value"
        variant="outline"
        size="icon-xs"
        :title="t('run.findInSession')"
        :aria-label="t('run.findInSession')"
        :aria-pressed="runFind.open.value"
        :class="{ 'bg-primary/15 text-primary border-primary/30': runFind.open.value }"
        @click="runFind.open.value ? runFind.closeFind() : runFind.openFind()"
      >
        <Search class="h-3.5 w-3.5" :stroke-width="1.75" />
      </Button>
    <template v-if="!isSessionWindow">
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
    </template>
    </div>
  </div>
</template>
