<script setup lang="ts">
// The run screen's header: the session's name (click to rename), then Bookmarks, Find, the flag chip and pop-out.
// A session's own window adds a ⋯ menu with the session's actions. Connections and links live in the info bar
// under the composer. State lives in useRunSession.
import { useI18n } from 'vue-i18n'
import { MoreHorizontal, Search, SquareArrowOutUpRight, SquareArrowDownLeft } from 'lucide-vue-next'
import RunActionItems from '@/components/RunActionItems.vue'
import { Button, DropdownMenu, DropdownSeparator } from '@/components/ui'
import SessionTitle from '@/components/sessions/SessionTitle.vue'
import SessionFlagChip from '@/components/sessions/SessionFlagChip.vue'
import SessionBookmarksMenu from '@/components/sessions/SessionBookmarksMenu.vue'
import { useRunSession } from './useRunSession'
import { onBeforeUnmount } from 'vue'
import { sessionHeaderMenu } from '@/lib/sessionHeaderMenu'
import { dockSessionToMain } from '@/lib/sessionWindow'
import { registerMergeBack } from '@/lib/windowsOverlay'
import { titleEditRequest } from './titleEditRequest'

const { t } = useI18n()
const {
  projectId, isSessionWindow, currentRun, popoutSession, runActionProps, runActionHandlers, runFind,
} = useRunSession()

const menuBlocks = sessionHeaderMenu({ isSessionWindow })

function dock() {
  if (currentRun.value) void dockSessionToMain(currentRun.value.id, projectId.value)
}

// The "Open windows" overlay of another window asks this session window to move back.
if (isSessionWindow) {
  const unregister = registerMergeBack(async () => {
    if (currentRun.value) await dockSessionToMain(currentRun.value.id, projectId.value)
  })
  onBeforeUnmount(unregister)
}
</script>

<template>
  <div class="flex items-center justify-between gap-2 px-3 h-12 border-b border-border bg-card shrink-0">
    <!-- flex-1 so the rename field can grow to the free space; the actions below are shrink-0. -->
    <div class="flex flex-1 items-center gap-2 min-w-0">
      <SessionTitle />
    </div>
    <!-- Icon-only actions; labels live in the tooltips so the header stays short no matter how narrow the
         pane gets. The flag chip comes last before pop-out so a long flag name never pushes the buttons around. -->
    <div class="flex items-center gap-1 shrink-0">
      <SessionBookmarksMenu />
      <!-- ⌘F opens the same bar (App.vue dispatches the key); this is its button. -->
      <Button
        v-if="runFind.enabled.value"
        variant="ghost"
        size="icon-sm"
        :title="t('run.findInSession')"
        :aria-label="t('run.findInSession')"
        :aria-pressed="runFind.open.value"
        :class="{ 'bg-primary/15 text-primary hover:text-primary': runFind.open.value }"
        @click="runFind.open.value ? runFind.closeFind() : runFind.openFind()"
      >
        <Search class="h-3.5 w-3.5" :stroke-width="1.75" />
      </Button>
      <SessionFlagChip />
      <!-- Pop out here; "back to main window" in the session's own window (QĐ-08). -->
      <Button
        v-if="currentRun"
        variant="ghost"
        size="icon-sm"
        :title="isSessionWindow ? t('run.dockToMain') : t('run.popOut')"
        :aria-label="isSessionWindow ? t('run.dockToMain') : t('run.popOut')"
        @click="isSessionWindow ? dock() : popoutSession(currentRun.id)"
      >
        <component :is="isSessionWindow ? SquareArrowDownLeft : SquareArrowOutUpRight" class="h-3.5 w-3.5" :stroke-width="1.75" />
      </Button>
      <!-- A session's own window has no History column, so its ⋯ menu carries the session's actions and
           Delete; the main window has no ⋯ (the History row has them). -->
      <DropdownMenu v-if="menuBlocks.length" align="right">
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
            <!-- This session (same list as its History row); Rename edits the header name in place. -->
            <RunActionItems
              v-if="block === 'runActions' && currentRun"
              v-bind="runActionProps(currentRun)"
              section="actions"
              v-on="runActionHandlers(currentRun, () => titleEditRequest++)"
            />

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
