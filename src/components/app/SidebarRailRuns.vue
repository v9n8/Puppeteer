<script setup lang="ts">
// The icon rail's stand-in for the Active runs dock: one icon with the number
// of active runs (amber when any waits for the user). Clicking opens the same
// list as a popup beside the rail.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Activity } from 'lucide-vue-next'
import ActiveRunsPopup from '@/components/activeRuns/ActiveRunsPopup.vue'
import { useActiveRunRows } from '@/composables/useActiveRunRows'

const { t } = useI18n()
// Same rows as the dock: a worker counts with its conductor, not on its own.
const { displayRows, waitingCount } = useActiveRunRows()
const total = computed(() => displayRows.value.length)

const tip = computed(() =>
  waitingCount.value > 0
    ? t('nav.rail.runsWaiting', { n: total.value, waiting: waitingCount.value })
    : t('nav.rail.runs', { n: total.value }),
)
</script>

<template>
  <ActiveRunsPopup v-if="total > 0" side="right">
    <button
      type="button"
      class="relative flex h-9 w-full cursor-pointer select-none items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      :title="tip"
      :aria-label="tip"
    >
      <Activity class="h-4 w-4" :stroke-width="1.75" />
      <span
        class="absolute -right-0.5 -top-0.5 flex h-4 min-w-[16px] items-center justify-center rounded-full px-1 text-2xs font-medium leading-none"
        :class="waitingCount > 0 ? 'bg-warning text-warning-foreground' : 'bg-primary text-primary-foreground'"
      >
        {{ total }}
      </span>
    </button>
  </ActiveRunsPopup>
</template>
