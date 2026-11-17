<script setup lang="ts">
// Running: the Sessions running or waiting for the user in every Project (tab
// "Session") and everything Puppeteer started in the background (tab "Background
// processes"). The tab lives in `?tab=`; `?run=` asks the processes tab to scroll
// to one Session's group.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { CircleStop } from 'lucide-vue-next'
import { Button, PageHeader } from '@/components/ui'
import RunningSessionsTab from '@/components/running/RunningSessionsTab.vue'
import BackgroundProcessesTab from '@/components/running/BackgroundProcessesTab.vue'
import { useRunningRows } from '@/composables/useRunningRows'
import { useBackgroundProcessesStore } from '@/stores/backgroundProcesses'
import { runningCounts } from '@/lib/runningSessions'

defineOptions({ name: 'RunningView' })

type Tab = 'session' | 'processes'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const { rows } = useRunningRows()
const procs = useBackgroundProcessesStore()
const sessionsTab = ref<InstanceType<typeof RunningSessionsTab> | null>(null)

const tab = computed<Tab>(() => (route.query.tab === 'processes' ? 'processes' : 'session'))
const focusRunId = computed(() => (typeof route.query.run === 'string' ? route.query.run : null))
// The Session tab stays mounted (its filter and selection survive a tab switch);
// the processes tab polls while shown, so it mounts only while chosen.

const sessionCount = computed(() => runningCounts(rows.value, null).all)
const processCount = computed(() => procs.counts.active + procs.counts.leftover + procs.counts.orphan)

const tabs = computed(() => [
  { id: 'session' as const, label: t('processes.running.tabSession'), count: sessionCount.value },
  { id: 'processes' as const, label: t('processes.running.tabProcesses'), count: processCount.value },
])

function select(id: Tab) {
  router.replace({ query: id === 'processes' ? { tab: 'processes' } : {} }).catch(() => {})
}

function viewProcesses(runId: string) {
  router.replace({ query: { tab: 'processes', run: runId } }).catch(() => {})
}
</script>

<template>
  <div class="flex h-full flex-col">
    <PageHeader :title="t('processes.running.title')">
      <template #actions>
        <Button
          v-if="tab === 'session' && sessionsTab?.canStopAll"
          variant="outline"
          :title="t('processes.running.action.stopAllTitle')"
          @click="sessionsTab.stopAll()"
        >
          <CircleStop class="h-3.5 w-3.5" :stroke-width="1.75" />
          {{ t('processes.running.action.stopAll') }}
        </Button>
      </template>
    </PageHeader>
    <div class="flex shrink-0 gap-1 border-b border-border/60 px-6" role="tablist">
      <button
        v-for="x in tabs"
        :key="x.id"
        type="button"
        role="tab"
        :aria-selected="tab === x.id"
        class="-mb-px flex h-9 cursor-pointer items-center gap-1.5 border-b-2 px-3 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        :class="tab === x.id
          ? 'border-primary text-foreground'
          : 'border-transparent text-muted-foreground hover:text-foreground'"
        @click="select(x.id)"
      >
        {{ x.label }}
        <span class="flex h-4 min-w-4 items-center justify-center rounded-full bg-muted px-1.5 text-2xs font-medium text-muted-foreground">{{ x.count }}</span>
      </button>
    </div>
    <div class="min-h-0 flex-1">
      <RunningSessionsTab ref="sessionsTab" v-show="tab === 'session'" class="h-full" @view-processes="viewProcesses" />
      <BackgroundProcessesTab v-if="tab === 'processes'" :focus-run-id="focusRunId" class="h-full" />
    </div>
  </div>
</template>
