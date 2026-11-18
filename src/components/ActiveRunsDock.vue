<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { Activity, ChevronDown, ChevronUp } from 'lucide-vue-next'
import ActiveRunRow from '@/components/activeRuns/ActiveRunRow.vue'
import ActiveRunsPopup from '@/components/activeRuns/ActiveRunsPopup.vue'
import { useActiveRunRows } from '@/composables/useActiveRunRows'
import { DOCK_MAX_ROWS, type DockMode } from '@/lib/dockMode'

/**
 * App-wide dock of all currently-active runs (streaming or awaiting a
 * permission / question), shown in the sidebar so the user can monitor and
 * jump between concurrent conversations on different projects from anywhere in
 * the app. It doubles as a permission center: a run blocked on input shows an
 * amber "needs permission" affordance that deep-links to its full modal in
 * RunView (we deliberately don't approve diffs blind from this narrow panel).
 *
 * `full` lists the runs (4 rows, then it scrolls); `compact` is one line whose
 * text opens the same list as a popup. The parent keeps the choice.
 */

defineProps<{ mode: DockMode }>()
defineEmits<{ toggle: [] }>()

const router = useRouter()
const { t } = useI18n()
const { displayRows, waitingCount, doneCount, activeRunId, isFailure, failureLabel, label, projectName, open } =
  useActiveRunRows()

function seeAll() {
  router.push('/running').catch(() => {})
}
</script>

<template>
  <div v-if="displayRows.length > 0" class="px-2 pb-2 border-t border-border pt-2">
    <!-- One bordered line: the text opens the popup, the chevron (laid over the
         line's right end, a separate button) expands the dock. -->
    <div v-if="mode === 'compact'" class="relative">
      <ActiveRunsPopup side="top">
        <button
          type="button"
          class="flex h-8 w-full min-w-0 cursor-pointer select-none items-center gap-2 rounded-sm border border-border pl-2 pr-8 text-left text-foreground transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <Activity class="h-3.5 w-3.5 shrink-0 text-muted-foreground" :stroke-width="2" />
          <span class="min-w-0 flex-1 truncate text-xs">
            {{ t('misc.activeRuns.compactLabel', { n: displayRows.length }, displayRows.length) }}
          </span>
          <span
            v-if="waitingCount > 0"
            class="flex h-4 min-w-[16px] items-center justify-center rounded-full bg-warning px-1 text-2xs font-medium text-warning-foreground leading-none"
          >{{ waitingCount }}</span>
          <span
            v-else-if="doneCount > 0"
            class="flex h-4 min-w-[16px] items-center justify-center rounded-full bg-success px-1 text-2xs font-medium text-success-foreground leading-none"
          >{{ doneCount }}</span>
        </button>
      </ActiveRunsPopup>
      <button
        type="button"
        class="absolute inset-y-px right-px flex w-7 cursor-pointer items-center justify-center rounded-r-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        :title="t('misc.activeRuns.expand')"
        :aria-label="t('misc.activeRuns.expand')"
        @click="$emit('toggle')"
      >
        <ChevronUp class="h-3.5 w-3.5" :stroke-width="2" />
      </button>
    </div>

    <template v-else>
      <div class="mx-1 mb-0.5 flex items-center gap-1.5 text-sm font-medium text-muted-foreground">
        <span class="flex h-7 min-w-0 flex-1 items-center">
          <span class="truncate">{{ t('misc.activeRuns.heading') }}</span>
        </span>
        <span
          v-if="waitingCount > 0"
          class="flex h-4 min-w-[16px] items-center justify-center rounded-full bg-warning px-1 text-2xs font-medium text-warning-foreground leading-none"
        >{{ waitingCount }}</span>
        <span
          v-else-if="doneCount > 0"
          class="flex h-4 min-w-[16px] items-center justify-center rounded-full bg-success px-1 text-2xs font-medium text-success-foreground leading-none"
        >{{ doneCount }}</span>
        <button
          type="button"
          class="flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          :title="t('misc.activeRuns.collapse')"
          :aria-label="t('misc.activeRuns.collapse')"
          @click="$emit('toggle')"
        >
          <ChevronDown class="h-3.5 w-3.5" :stroke-width="2" />
        </button>
      </div>

      <!-- 4 rows tall (about 44px each), then it scrolls. -->
      <div class="space-y-0.5 max-h-[176px] overflow-y-auto">
        <ActiveRunRow
          v-for="item in displayRows"
          :key="item.row.runId"
          :item="item"
          :active="item.row.runId === activeRunId"
          :label="label(item.row)"
          :project="projectName(item.row.projectId)"
          :failure-text="failureLabel(item.row)"
          :failed="isFailure(item.row.status)"
          @open="open(item.row)"
        />
      </div>
      <button
        v-if="displayRows.length > DOCK_MAX_ROWS"
        type="button"
        class="mt-0.5 w-full cursor-pointer select-none rounded-sm px-2 py-1.5 text-left text-xs text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        @click="seeAll"
      >
        {{ t('misc.activeRuns.seeAllCount', { n: displayRows.length }) }}
      </button>
    </template>
  </div>
</template>
