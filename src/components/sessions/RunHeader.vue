<script setup lang="ts">
// The run screen's header: the session's name (click to rename), the Bookmarks and Find buttons and the ⋯ menu.
// Connections and links live in the info bar under the composer. State lives in useRunSession.
import { useI18n } from 'vue-i18n'
import {
  Terminal, Settings, Code2, Folder, FolderOpen, Maximize2, Minimize2, Check, MoreHorizontal, Search,
  SquareArrowOutUpRight, SquareArrowDownLeft,
} from 'lucide-vue-next'
import RunActionItems from '@/components/RunActionItems.vue'
import { Button, DropdownMenu, DropdownItem, DropdownSeparator } from '@/components/ui'
import SessionTitle from '@/components/sessions/SessionTitle.vue'
import SessionFlagChip from '@/components/sessions/SessionFlagChip.vue'
import SessionBookmarksMenu from '@/components/sessions/SessionBookmarksMenu.vue'
import { useRunSession } from './useRunSession'
import { computed } from 'vue'
import { emit as emitEvent } from '@tauri-apps/api/event'
import { projectPageQuery, type ProjectSectionId } from '@/lib/projectSections'
import { sessionHeaderMenu } from '@/lib/sessionHeaderMenu'
import { dockSessionToMain } from '@/lib/sessionWindow'
import { SCREEN_OPEN_ROUTE, type ScreenOpenRoute } from '@/lib/detachedScreens'
import { titleEditRequest } from './titleEditRequest'

const { t } = useI18n()
const {
  router, uiLayout, projectId, project, isSessionWindow, currentRun, handleOpenInVscode,
  handleOpenInFolder, handleOpenInTerminal, popoutSession, runActionProps, runActionHandlers, runFind,
} = useRunSession()

const menuBlocks = computed(() => sessionHeaderMenu({ isSessionWindow, hasProject: !!project.value }))

// "View project" / "Project settings" open the Project screen. A session window has no router: the main
// window opens it (and comes forward).
function openProjectScreen(section?: ProjectSectionId) {
  const query = projectPageQuery(projectId.value, section)
  if (isSessionWindow) {
    const path = `/projects?${new URLSearchParams(query)}`
    void emitEvent(SCREEN_OPEN_ROUTE, { path } satisfies ScreenOpenRoute)
  } else {
    void router.push({ path: '/projects', query })
  }
}

function dock() {
  if (currentRun.value) void dockSessionToMain(currentRun.value.id, projectId.value)
}
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
      <!-- Pop out here; "back to main window" in the session's own window (QĐ-08). -->
      <Button
        v-if="currentRun"
        variant="outline"
        size="icon"
        :title="isSessionWindow ? t('run.dockToMain') : t('run.popOut')"
        :aria-label="isSessionWindow ? t('run.dockToMain') : t('run.popOut')"
        @click="isSessionWindow ? dock() : popoutSession(currentRun.id)"
      >
        <component :is="isSessionWindow ? SquareArrowDownLeft : SquareArrowOutUpRight" class="h-3.5 w-3.5" :stroke-width="1.75" />
      </Button>
      <!-- One ⋯ menu for the rest; the blocks come from sessionHeaderMenu: the main window's is short
           (the History row has the session's actions), a session window's carries them too. -->
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
          <template v-for="(block, i) in menuBlocks" :key="block">
            <DropdownSeparator v-if="i > 0 && block !== 'delete'" />
            <template v-if="block === 'openProjectIn'">
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
            </template>

            <!-- This session (same list as its History row); Rename edits the header name in place. -->
            <RunActionItems
              v-else-if="block === 'runActions' && currentRun"
              v-bind="runActionProps(currentRun)"
              section="actions"
              v-on="runActionHandlers(currentRun, () => titleEditRequest++)"
            />

            <DropdownItem
              v-else-if="block === 'focusMode'"
              :title="uiLayout.focusMode ? t('run.exitFocusMode') : t('run.enterFocusMode')"
              @click="uiLayout.toggleFocus()"
            >
              <component :is="uiLayout.focusMode ? Minimize2 : Maximize2" class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
              <span class="flex-1">{{ t('run.focusMode') }}</span>
              <Check v-if="uiLayout.focusMode" class="h-3.5 w-3.5 shrink-0 text-primary" :stroke-width="2" />
            </DropdownItem>

            <DropdownItem v-else-if="block === 'viewProject'" @click="openProjectScreen()">
              <Folder class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
              {{ t('run.viewProject') }}
            </DropdownItem>
            <DropdownItem
              v-else-if="block === 'projectSettings'"
              :title="t('run.projectSettingsTitle')"
              @click="openProjectScreen('session')"
            >
              <Settings class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
              {{ t('run.projectSettings') }}
            </DropdownItem>

            <RunActionItems
              v-else-if="block === 'delete' && currentRun"
              v-bind="runActionProps(currentRun)"
              section="delete"
              v-on="runActionHandlers(currentRun)"
            />
          </template>
        </div>
      </DropdownMenu>
    </div>
  </div>
</template>
