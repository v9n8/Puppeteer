<script setup lang="ts">
// The icon rail's stand-in for the Active runs dock: one icon with the number
// of active runs (amber when any waits for the user). Clicking expands the
// sidebar, where the dock lives.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Activity } from 'lucide-vue-next'
import { useLiveRunsStore } from '@/stores/liveRuns'
import { useRunsStore } from '@/stores/runs'
import { useUILayoutStore } from '@/stores/uiLayout'

const { t } = useI18n()
const live = useLiveRunsStore()
const runsStore = useRunsStore()
const uiLayout = useUILayoutStore()

// Same rule as the dock: running or blocked on input; a worker counts with its
// conductor, not on its own.
const counts = computed(() => {
  let total = 0
  let waiting = 0
  live.sessions.forEach((s) => {
    if (runsStore.runMeta.get(s.runId)?.conductor_run_id) return
    const pending = s.permissionQueue.length > 0
    if (s.status !== 'running' && !pending) return
    total += 1
    if (pending) waiting += 1
  })
  return { total, waiting }
})

const tip = computed(() =>
  counts.value.waiting > 0
    ? t('nav.rail.runsWaiting', { n: counts.value.total, waiting: counts.value.waiting })
    : t('nav.rail.runs', { n: counts.value.total }),
)
</script>

<template>
  <button
    v-if="counts.total > 0"
    type="button"
    class="relative flex h-9 w-full cursor-pointer select-none items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    :title="tip"
    :aria-label="tip"
    @click="uiLayout.expandSidebar()"
  >
    <Activity class="h-4 w-4" :stroke-width="1.75" />
    <span
      class="absolute -right-0.5 -top-0.5 flex h-4 min-w-[16px] items-center justify-center rounded-full px-1 text-2xs font-medium leading-none"
      :class="counts.waiting > 0 ? 'bg-warning text-warning-foreground' : 'bg-primary text-primary-foreground'"
    >
      {{ counts.total }}
    </span>
  </button>
</template>
