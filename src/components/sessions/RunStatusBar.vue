<script setup lang="ts">
// The session info bar under the composer (V3-16), in the session column: plan
// usage on the left; on the right the context, then Connections and Links. A
// session not yet started (nothing sent) shows only the usage rings.
// RunView shows it with the composer. State lives in useRunSession.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import UsageRings from '@/components/UsageRings.vue'
import SessionConnectionsMenu from './SessionConnectionsMenu.vue'
import SessionContextChip from './SessionContextChip.vue'
import SessionLinksMenu from './SessionLinksMenu.vue'
import { useRunSession } from './useRunSession'

const { t } = useI18n()
const {
  features, contextTokens, currentStatus, connectionGroups,
} = useRunSession()

const started = computed(() => currentStatus.value !== 'fetched')
const hasMenus = computed(
  () => connectionGroups.value.length > 0
    || features.allows('run.linkedSlack') || features.allows('run.linkedTodos') || features.allows('run.linkedNotes'),
)
</script>

<template>
  <footer
    class="flex h-8 shrink-0 items-center gap-1 border-t border-border bg-card/40 px-2 min-w-0"
    :aria-label="t('run.statusBar.label')"
  >
    <!-- Usage slot: one plan usage ring per account (UX-23). Unmounting the
         rings also stops their 60 s refresh timer and the quota toast. -->
    <div class="flex min-w-0 flex-1 items-center">
      <UsageRings v-if="features.isOn('usage.context_meter')" />
    </div>

    <template v-if="started">
      <SessionContextChip />
      <span v-if="contextTokens > 0 && hasMenus" class="h-3 w-px shrink-0 bg-border" />
      <SessionConnectionsMenu />
      <SessionLinksMenu />
    </template>
  </footer>
</template>
