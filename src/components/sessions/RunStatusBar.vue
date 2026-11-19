<script setup lang="ts">
// The session info bar under the composer (V3-16), in the session column: plan
// usage on the left; on the right the context, then Connections and Links. A
// session not yet started (nothing sent) shows only the usage block.
// RunView shows it with the composer. State lives in useRunSession.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import UsageRings from '@/components/UsageRings.vue'
import { currentUsageKey } from '@/lib/usageRing'
import SessionConnectionsMenu from './SessionConnectionsMenu.vue'
import SessionContextChip from './SessionContextChip.vue'
import SessionLinksMenu from './SessionLinksMenu.vue'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const {
  features, contextTokens, currentStatus, connectionGroups,
  currentRun, effectiveEngine, effectiveProjectClaudeAccountId, codexStore,
} = useRunSession()

// The usage block leads with the account the account chip shows (DS-1).
const usageCurrentKey = computed(() =>
  currentUsageKey(currentRun.value, { engine: effectiveEngine.value, claudeAccountId: effectiveProjectClaudeAccountId.value }),
)
// ★: the Claude account a run with no pick gets, and the shared Codex login
// once there are Codex accounts to pick instead.
const usageDefaultKeys = computed(() => [
  ...(effectiveProjectClaudeAccountId.value ? [`claude:${effectiveProjectClaudeAccountId.value}`] : []),
  ...(codexStore.accounts.length > 0 ? ['codex'] : []),
])

const started = computed(() => currentStatus.value !== 'fetched')
const hasMenus = computed(
  () => connectionGroups.value.length > 0
    || features.allows('run.linkedSlack') || features.allows('run.linkedTodos') || features.allows('run.linkedNotes'),
)
</script>

<template>
  <footer
    class="flex h-8 shrink-0 items-center gap-0.5 border-t border-border bg-background px-2 min-w-0"
    :aria-label="t('run.statusBar.label')"
  >
    <!-- Usage slot: one plan usage block for every account (UX-23, DS-1).
         Unmounting it also stops its 60 s refresh timer and the quota toast. -->
    <div class="flex min-w-0 flex-1 items-center">
      <UsageRings v-if="features.isOn('usage.context_meter')" :current-key="usageCurrentKey" :default-keys="usageDefaultKeys" />
    </div>

    <template v-if="started">
      <SessionContextChip />
      <span v-if="contextTokens > 0 && hasMenus" class="h-3 w-px shrink-0 bg-border" />
      <SessionConnectionsMenu />
      <SessionLinksMenu />
    </template>
  </footer>
</template>
