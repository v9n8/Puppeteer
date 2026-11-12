<script setup lang="ts">
/**
 * Inbox ("needs attention") — the first screen of a paired phone, built for
 * "open, check, lock again":
 *
 *  - every permission prompt / AskUserQuestion still open on the Host, across
 *    ALL runs, answerable in place (4-button grid);
 *  - below it, runs that may be stuck (watchdog), then the runs that finished,
 *    failed or were stopped by the system (app closed, quota) since the user
 *    last had the app open, with a dot until opened. A user Stop and Conductor
 *    workers' turns are left out.
 *
 * Backend-agnostic like the other controller views: state in via props, intent
 * out via events. Ordering / "since last visit" live in `@/lib/remoteInbox`.
 */
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle, CheckCircle2, CircleStop, XCircle } from 'lucide-vue-next'
import type { PermissionRequest } from '@/components/PermissionPrompt.vue'
import {
  agoParts,
  finishedKind,
  inboxRuns,
  parseHostTime,
  type StuckNotices,
} from '@/lib/remoteInbox'
import ControllerPermissionPrompt from './ControllerPermissionPrompt.vue'
import type { ConnStatus } from '../connection'
import type { ProjectInfo, QuestionAnswers, RemoteDecision, RunInfo } from '../protocol'

const props = defineProps<{
  status: ConnStatus
  /** Every open prompt, already in display order. */
  pending: PermissionRequest[]
  /** request id → when this device first saw it (ms). */
  pendingSeenAt: Record<string, number>
  runs: RunInfo[]
  projects: ProjectInfo[]
  /** "Since last visit" anchor (ms). */
  sinceMs: number
  /** Finished runs already opened (no unread dot). */
  readIds: string[]
  /** Runs the watchdog says may be stuck. */
  stuck: StuckNotices
  /** The Host accepts Always from a Controller. */
  allowAlways?: boolean
}>()

const emit = defineEmits<{
  decide: [request: PermissionRequest, decision: RemoteDecision]
  answer: [request: PermissionRequest, answers: QuestionAnswers]
  open: [runId: string]
}>()

const { t, locale } = useI18n()

const isActive = computed(() => props.status === 'connected')

// Relative times ("2 min ago") drift while the screen stays open.
const now = ref(Date.now())
let tick: ReturnType<typeof setInterval> | null = null
onMounted(() => {
  tick = setInterval(() => (now.value = Date.now()), 30_000)
})
onUnmounted(() => {
  if (tick) clearInterval(tick)
})

const rtf = computed(() => new Intl.RelativeTimeFormat(locale.value, { numeric: 'auto' }))
function ago(ms: number): string {
  const { value, unit } = agoParts(ms, now.value)
  return rtf.value.format(value, unit)
}

const runsById = computed(() => new Map(props.runs.map((r) => [r.id, r])))

function projectName(run: RunInfo | undefined): string {
  if (!run) return t('controller.browser.ungrouped')
  const own = run.project_name?.trim()
  if (own) return own
  return props.projects.find((p) => p.id === run.project_id)?.name?.trim() || t('controller.browser.ungrouped')
}

function runTitle(run: RunInfo | undefined, runId: string): string {
  const title = run?.title?.trim()
  return title || t('controller.browser.untitled', { id: runId.slice(0, 8) })
}

/** "project · run · 2 min ago" for a prompt card. */
function contextOf(req: PermissionRequest): string {
  const run = runsById.value.get(req.run_id)
  const parts = [projectName(run), runTitle(run, req.run_id)]
  const seen = props.pendingSeenAt[req.request_id]
  if (seen) parts.push(ago(seen))
  return parts.join(' · ')
}

const finished = computed(() => inboxRuns(props.runs, props.sinceMs, props.stuck))
const read = computed(() => new Set(props.readIds))

function kindOf(run: RunInfo) {
  return finishedKind(run.status, run.end_reason)
}

/** i18n key of a row's status line; a row listed only as stuck is still running. */
function statusKey(run: RunInfo): string {
  switch (kindOf(run)) {
    case 'success':
      return 'finishedOk'
    case 'failure':
      return 'finishedFailed'
    case 'stopped':
      return run.end_reason === 'quota' ? 'stoppedQuota' : 'interrupted'
    default:
      return 'lastActive'
  }
}

function finishedLabel(run: RunInfo): string {
  const at = parseHostTime(run.updated_at) ?? parseHostTime(run.created_at) ?? now.value
  return `${projectName(run)} · ${t(`controller.inbox.${statusKey(run)}`, { ago: ago(at) })}`
}

const sinceLabel = computed(() =>
  new Date(props.sinceMs).toLocaleString(locale.value, {
    weekday: 'short',
    hour: '2-digit',
    minute: '2-digit',
  }),
)
</script>

<template>
  <div class="relative flex h-full min-h-0 flex-col bg-background text-foreground">

    <main class="flex-1 min-h-0 overflow-auto">
      <!-- ── Waiting for you ─────────────────────────────────────────────── -->
      <h2 class="px-3 pt-3 pb-1.5 text-2xs font-medium uppercase tracking-wider text-foreground/50">
        {{ t('controller.inbox.waiting', { count: pending.length }) }}
      </h2>
      <p v-if="!pending.length" class="px-3 py-4 text-center text-sm text-foreground/50">
        {{ isActive ? t('controller.inbox.nothingWaiting') : t('controller.inbox.loading') }}
      </p>
      <div v-else class="px-3 space-y-3">
        <ControllerPermissionPrompt
          v-for="req in pending"
          :key="req.request_id"
          :request="req"
          variant="inbox"
          :context="contextOf(req)"
          :allow-always="allowAlways"
          @decide="emit('decide', req, $event)"
          @answer="emit('answer', req, $event)"
          @open="emit('open', req.run_id)"
        />
      </div>

      <!-- ── Since last visit ────────────────────────────────────────────── -->
      <h2
        class="mt-4 px-3 pt-3 pb-1.5 flex items-baseline justify-between gap-2 text-2xs font-medium uppercase tracking-wider text-foreground/50 border-t border-border/40"
      >
        <span>{{ t('controller.inbox.sinceLastVisit') }}</span>
        <span class="normal-case tracking-normal font-normal">{{ sinceLabel }}</span>
      </h2>
      <p v-if="!finished.length" class="px-3 py-4 text-center text-sm text-foreground/50">
        {{ t('controller.inbox.noneFinished') }}
      </p>
      <button
        v-for="run in finished"
        :key="run.id"
        class="w-full flex items-center gap-2.5 px-3 py-2.5 text-left border-b border-border/40 hover:bg-accent transition-colors cursor-pointer"
        :title="t('controller.browser.openRun')"
        @click="emit('open', run.id)"
      >
        <XCircle
          v-if="kindOf(run) === 'failure'"
          class="shrink-0 h-4 w-4 text-destructive"
          :stroke-width="2"
        />
        <CircleStop
          v-else-if="kindOf(run) === 'stopped'"
          class="shrink-0 h-4 w-4 text-warning"
          :stroke-width="2"
        />
        <CheckCircle2
          v-else-if="kindOf(run) === 'success'"
          class="shrink-0 h-4 w-4 text-success"
          :stroke-width="2"
        />
        <AlertTriangle v-else class="shrink-0 h-4 w-4 text-warning" :stroke-width="2" />
        <span class="min-w-0 flex-1">
          <span class="block text-sm truncate">{{ runTitle(run, run.id) }}</span>
          <span class="block text-2xs text-foreground/45 truncate">{{ finishedLabel(run) }}</span>
        </span>
        <span
          v-if="run.id in stuck"
          class="shrink-0 rounded-md bg-warning/15 px-1.5 py-0.5 text-2xs font-medium text-warning"
        >
          {{ t('controller.inbox.mayBeStuck') }}
        </span>
        <span
          v-if="kindOf(run) && !read.has(run.id)"
          class="shrink-0 h-2 w-2 rounded-full bg-primary"
          :title="t('controller.inbox.unread')"
        />
      </button>
    </main>
  </div>
</template>
