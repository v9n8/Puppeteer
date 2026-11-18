<script setup lang="ts">
// One run in the Active runs dock and its popup: status dot, title, Project,
// worker count, and the Review / Stopped / Failed / Done tag. The parent owns
// the labels and what a click does (composables/useActiveRunRows).
import { useI18n } from 'vue-i18n'
import { ShieldAlert, CheckCircle2, XCircle, Network, AlertTriangle } from 'lucide-vue-next'
import type { DisplayRow } from '@/composables/useActiveRunRows'

defineProps<{
  item: DisplayRow
  /** The run on screen. */
  active: boolean
  label: string
  project: string
  /** "Stopped" / "Failed" / "Stopped: quota", for a finished run that did not succeed. */
  failureText: string
  failed: boolean
}>()
defineEmits<{ open: [] }>()

const { t } = useI18n()
</script>

<template>
  <button
    type="button"
    data-run-item
    class="w-full flex items-center gap-2 py-1 px-2 rounded-sm text-left transition-colors cursor-pointer select-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    :class="active
      ? 'bg-accent text-foreground'
      : 'text-muted-foreground hover:text-foreground hover:bg-accent/50'"
    :title="project + ' — ' + label"
    @click="$emit('open')"
  >
    <span class="relative flex h-2 w-2 shrink-0">
      <span
        v-if="item.row.status === 'running'"
        class="absolute inline-flex h-full w-full rounded-full bg-primary opacity-60 animate-ping"
      />
      <span
        class="relative inline-flex h-2 w-2 rounded-full"
        :class="item.row.pending > 0
          ? 'bg-warning'
          : item.row.status === 'running'
            ? 'bg-primary'
            : item.row.done
              ? (failed ? 'bg-destructive' : 'bg-success')
              : 'bg-muted-foreground/40'"
      />
    </span>
    <span class="min-w-0 flex-1">
      <span class="block truncate text-sm">{{ label }}</span>
      <span class="block truncate text-2xs text-muted-foreground/70">{{ project }}</span>
    </span>
    <span
      v-if="item.row.stuck"
      class="shrink-0 text-warning"
      role="img"
      :title="t('misc.activeRuns.mayBeStuck')"
      :aria-label="t('misc.activeRuns.mayBeStuck')"
    >
      <AlertTriangle class="h-3.5 w-3.5" :stroke-width="2" />
    </span>
    <!-- Active-worker count for a conductor. -->
    <span
      v-if="item.isGroup"
      class="shrink-0 flex items-center gap-0.5 rounded-full bg-primary/10 px-1.5 py-0.5 text-2xs font-medium text-primary"
      :title="t('conductor.workerCountShort', { n: item.workerCount })"
    >
      <Network class="h-2.5 w-2.5" :stroke-width="2" />
      {{ item.workerCount }}
    </span>
    <span
      v-if="item.row.pending > 0"
      class="flex items-center gap-0.5 rounded bg-warning/15 px-1 py-0.5 text-2xs font-medium text-warning shrink-0"
    >
      <ShieldAlert class="h-3 w-3" :stroke-width="2" />
      {{ t('misc.activeRuns.review') }}
    </span>
    <span
      v-else-if="item.row.done && failed"
      class="flex items-center gap-0.5 rounded bg-destructive/15 px-1 py-0.5 text-2xs font-medium text-destructive shrink-0"
    >
      <XCircle class="h-3 w-3" :stroke-width="2" />
      {{ failureText }}
    </span>
    <span
      v-else-if="item.row.done"
      class="flex items-center gap-0.5 rounded bg-success/15 px-1 py-0.5 text-2xs font-medium text-success shrink-0"
    >
      <CheckCircle2 class="h-3 w-3" :stroke-width="2" />
      {{ t('misc.activeRuns.done') }}
    </span>
  </button>
</template>
