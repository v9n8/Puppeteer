<script setup lang="ts">
// The actions on one run / session, as DropdownItems — shared by the History
// row's ⋯ menu and the RunView header's ⋯ menu, so both offer the same list with
// the same rules. RunView owns the handlers (and their busy state); this only
// renders and emits.
//
//   section 'actions'  Pop out (Board only) · Copy path · Reveal in Finder · Open on GitHub · Refetch |
//                      Copy session ID · Re-sync (a session with a transcript, not running) |
//                      Flag ▸ · Fork · Rename · Pin · Protect
// Copy path / Reveal point at the session's own file (its log, RT-17).
//   section 'delete'   | Delete (hidden while the run is running)
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  AppWindow,
  Copy,
  ExternalLink,
  FolderOpen,
  Flag,
  GitFork,
  Hash,
  Pencil,
  Pin,
  PinOff,
  RefreshCw,
  RotateCcw,
  Shield,
  ShieldOff,
  Trash2,
} from 'lucide-vue-next'
import { DropdownItem, DropdownSeparator, DropdownSubmenu, InfoTip } from '@/components/ui'
import FlagMenuItems from '@/components/sessions/FlagMenuItems.vue'
import type { RunRecord } from '@/stores/runs'
import { useSessionFlagsStore } from '@/stores/sessionFlags'
import { flagById, flagLabel } from '@/lib/sessionFlags'
import { canForkRun } from '@/lib/runFork'
import { platformOf, revealLabelKey } from '@/lib/sessionPaths'

const props = defineProps<{
  run: RunRecord
  section: 'actions' | 'delete'
  /** A session pop-out can't pop itself out again. */
  isSessionWindow: boolean
  /** Offer "Open in new window": only the Board, which has no other way to pop out (RT-17). */
  canPopout?: boolean
  /** The run's issue / PR on GitHub, when any. */
  githubUrl: string | null
  /** An action for this run is in flight (its item is disabled meanwhile). */
  busy: { refetching: boolean; pinning: boolean; protecting: boolean; deleting: boolean; forking: boolean; resyncing?: boolean }
  /** A prop, not an emit: it must still run after the menu (and this component) is gone. */
  onFlagChanged?: () => void
}>()

const emit = defineEmits<{
  popout: []
  copyLogPath: []
  revealLog: []
  openGithub: []
  refetch: []
  copyId: []
  resync: []
  fork: []
  rename: []
  togglePin: []
  toggleProtect: []
  delete: []
}>()

const { t } = useI18n()
const revealKey = revealLabelKey(platformOf(navigator.userAgent))
const flagsStore = useSessionFlagsStore()
const currentFlagName = computed(() => {
  const flag = flagById(flagsStore.flagsOf(props.run.project_id), props.run.flag_id)
  return flag ? flagLabel(flag, t) : ''
})
</script>

<template>
  <template v-if="section === 'actions'">
    <!-- Pop this session out into its own OS window (one per run). Only on the
         Board (History and the header have their own button); hidden inside a
         session window — can't re-pop what's popped. -->
    <DropdownItem v-if="canPopout && !isSessionWindow" @click="emit('popout')">
      <AppWindow class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
      {{ t('run.popOutSession') }}
    </DropdownItem>
    <!-- The session's own file (.puppeteer/runs/<id>.log). -->
    <DropdownItem @click="emit('copyLogPath')">
      <Copy class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
      {{ t('run.copyWorkDir') }}
    </DropdownItem>
    <DropdownItem @click="emit('revealLog')">
      <FolderOpen class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
      {{ t(revealKey) }}
    </DropdownItem>
    <DropdownItem v-if="githubUrl" @click="emit('openGithub')">
      <ExternalLink class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
      {{ run.run_type === 'analyze_issue' ? t('run.openIssueOnGithub') : t('run.openPrOnGithub') }}
    </DropdownItem>
    <DropdownItem
      v-if="run.run_type !== 'session' && run.status !== 'running'"
      :disabled="busy.refetching"
      @click="emit('refetch')"
    >
      <RefreshCw class="h-3.5 w-3.5 shrink-0" :class="{ 'animate-spin': busy.refetching }" :stroke-width="1.75" />
      {{ run.run_type === 'analyze_issue' ? t('run.refetchIssue') : t('run.refetchPr') }}
    </DropdownItem>
    <DropdownSeparator />
    <DropdownItem @click="emit('copyId')">
      <Hash class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
      {{ t('run.copySessionId') }}
    </DropdownItem>
    <!-- Rebuild the log from the engine's transcript now (RT-16); asks first. -->
    <DropdownItem
      v-if="run.run_type === 'session' && run.session_id && run.status !== 'running'"
      :disabled="busy.resyncing"
      @click="emit('resync')"
    >
      <RotateCcw class="h-3.5 w-3.5 shrink-0" :class="{ 'animate-spin': busy.resyncing }" :stroke-width="1.75" />
      {{ t('run.resync.action') }}
    </DropdownItem>
    <DropdownSeparator />
    <!-- Flag ▸ (V3-19): the same menu as the header chip. -->
    <DropdownSubmenu>
      <template #label>
        <Flag class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
        {{ t('sessionFlags.menu.set') }}
      </template>
      <template #hint>{{ currentFlagName }}</template>
      <FlagMenuItems :run-id="run.id" :project-id="run.project_id" :flag-id="run.flag_id" :on-changed="() => props.onFlagChanged?.()" />
    </DropdownSubmenu>
    <!-- A new session with this one's conversation and settings (UX-28). -->
    <DropdownItem v-if="canForkRun(run)" :disabled="busy.forking" @click="emit('fork')">
      <GitFork class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
      {{ t('run.fork.action') }}
    </DropdownItem>
    <DropdownItem @click="emit('rename')">
      <Pencil class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
      {{ t('run.rename') }}
    </DropdownItem>
    <DropdownItem :disabled="busy.pinning" @click="emit('togglePin')">
      <component :is="run.pinned ? PinOff : Pin" class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
      {{ run.pinned ? t('run.unpinFromTop') : t('run.pinToTop') }}
    </DropdownItem>
    <!-- The ⓘ sits beside the item, not inside it: a button can't nest in the
         item's button, and a click on it must neither toggle protection nor
         close the menu (the panel closes on any click that reaches it). -->
    <div class="flex items-center">
      <DropdownItem class="min-w-0 flex-1" :disabled="busy.protecting" @click="emit('toggleProtect')">
        <component :is="run.protected ? ShieldOff : Shield" class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
        {{ run.protected ? t('run.unprotect') : t('run.protect') }}
      </DropdownItem>
      <span class="inline-flex px-1.5" @click.stop>
        <InfoTip :text="t('run.protectHelp')" />
      </span>
    </div>
  </template>

  <template v-else-if="run.status !== 'running'">
    <DropdownSeparator />
    <DropdownItem variant="destructive" :disabled="busy.deleting" @click="emit('delete')">
      <Trash2 class="h-3.5 w-3.5 shrink-0" :stroke-width="1.75" />
      {{ t('run.deleteThisRun') }}
    </DropdownItem>
  </template>
</template>
