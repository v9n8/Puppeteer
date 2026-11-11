<script setup lang="ts">
// The session status bar at the bottom of the run screen (UX-22): account plan
// usage on the left, then the context, project and account · model chips.
// RunView shows it with the composer. State lives in useRunSession.
import { useI18n } from 'vue-i18n'
import { Folder, FoldVertical } from 'lucide-vue-next'
import ContextMeter from '@/components/ContextMeter.vue'
import UsageRings from '@/components/UsageRings.vue'
import { DropdownMenu, DropdownItem } from '@/components/ui'
import { useProjectQuickView } from '@/composables/useProjectQuickView'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const quickView = useProjectQuickView()
const {
  features, project, isSessionWindow, contextTokens, contextModel,
  contextRateLimit, sendSlashCommand, sendingFollowUp, accountMenuOpen, toggleAccountMenu,
  engineBadgeIcon, accountModelLabel,
} = useRunSession()

const CHIP =
  'inline-flex h-5 min-w-0 shrink-0 items-center gap-1.5 rounded px-1.5 text-2xs text-muted-foreground '
  + 'transition-colors cursor-pointer hover:bg-muted hover:text-foreground '
  + 'disabled:cursor-default disabled:hover:bg-transparent disabled:hover:text-muted-foreground'

// Opens the project's quick view (UX-52) over the session. Only the main window
// hosts it, so in a session pop-out the chip only shows the name.
function openProject() {
  if (project.value) quickView.open(project.value.id)
}
</script>

<template>
  <footer
    class="flex h-7 shrink-0 items-center gap-1 border-t border-border bg-card/40 px-2 min-w-0"
    :aria-label="t('run.statusBar.label')"
  >
    <!-- Usage slot: one plan usage ring per account (UX-23). Unmounting the
         rings also stops their 60 s refresh timer and the quota toast. -->
    <div class="flex min-w-0 flex-1 items-center">
      <UsageRings v-if="features.isOn('usage.context_meter')" />
    </div>

    <!-- Context: this run's context window; the menu holds /compact. Hidden
         until the run has used any context, like the meter itself. -->
    <template v-if="contextTokens > 0">
      <DropdownMenu align="right" prefer="above">
        <template #trigger="{ open }">
          <button
            type="button"
            :class="CHIP"
            :title="t('run.statusBar.contextTitle')"
            aria-haspopup="menu"
            :aria-expanded="open"
          >
            <ContextMeter :tokens="contextTokens" :model="contextModel" :rate-limit="contextRateLimit" />
          </button>
        </template>
        <DropdownItem
          :disabled="sendingFollowUp"
          :title="t('misc.context.compactTitle')"
          @click="sendSlashCommand('/compact')"
        >
          <FoldVertical class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
          {{ t('run.statusBar.compact') }}
        </DropdownItem>
      </DropdownMenu>
      <span class="h-3 w-px shrink-0 bg-border" />
    </template>

    <button
      type="button"
      :class="CHIP"
      :disabled="isSessionWindow"
      :title="isSessionWindow ? undefined : t('run.statusBar.projectTitle')"
      @click="openProject"
    >
      <Folder class="h-3 w-3 shrink-0" :stroke-width="1.75" />
      <span class="max-w-40 truncate">{{ project?.name ?? t('run.projectFallback') }}</span>
    </button>
    <span class="h-3 w-px shrink-0 bg-border" />

    <!-- Account · model: opens the composer's account · model menu (the same
         one as its chip; data-account-menu-trigger keeps that menu's
         outside-click from closing it before this click toggles it). -->
    <button
      type="button"
      :class="CHIP"
      data-account-menu-trigger
      :title="t('run.statusBar.accountModelTitle')"
      aria-haspopup="dialog"
      :aria-expanded="accountMenuOpen"
      @click="toggleAccountMenu"
    >
      <component :is="engineBadgeIcon" class="h-3 w-3 shrink-0" :stroke-width="1.75" />
      <span class="max-w-60 truncate">{{ accountModelLabel }}</span>
    </button>
  </footer>
</template>
