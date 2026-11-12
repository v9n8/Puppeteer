<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { getAllWebviewWindows } from '@tauri-apps/api/webviewWindow'
import { useLiveRunsStore } from '@/stores/liveRuns'
import { useProjectsStore } from '@/stores/projects'
import { useRunsStore } from '@/stores/runs'
import { useWorkspaceTabsStore } from '@/stores/workspaceTabs'
import { useAppSettingsStore } from '@/stores/appSettings'
import type { PermissionRequest } from '@/components/PermissionPrompt.vue'
import { parseDesktopNotify } from '@/lib/desktopNotify'
import {
  allowsRunNotification,
  finishedRunKind,
  runNotificationText,
  type RunNotificationKind,
} from '@/lib/runNotificationText'
import { newStuckAlerts, type RunAttentionState, type StuckAlert } from '@/lib/stuckNotify'
import { EVT_SCHEDULE_FAILED } from '@/lib/schedules'
import { isRemoteMode } from '@/lib/ipc/remote'

/**
 * Headless, app-wide notifier for pending permission / question requests on runs
 * the user is NOT currently looking at. It renders nothing: in-app, the History
 * list shows an animated attention icon on the waiting run (see RunView), so the
 * only job here is firing a native OS notification when the app is in the
 * background. Clicking it focuses the window and routes to the waiting run, where
 * the in-place permission / question modal is shown.
 *
 * It also fires a native "session finished" notification when a main run (not a
 * conductor worker) completes a turn while the app window is unfocused; clicking
 * it opens that run the same way.
 *
 * It also fires a "session may be stuck" notification, once per alert, when the
 * backend watchdog says a run's sidecar stopped responding or never started
 * (lib/stuckNotify) — for conductor workers too, since a conductor cannot
 * revive a dead sidecar. A cleared alert notifies nothing.
 *
 * It also fires a "<schedule> did not start" notification when a schedule came
 * due but could not start its session (`schedule:failed`); clicking it opens
 * the Schedules screen. A schedule skipped because its last session is still
 * running sends no event.
 *
 * Every kind obeys its `desktop_notify` switch (Settings → Sessions →
 * Notifications), and the text follows the app's language (lib/runNotificationText).
 */

interface PendingRun {
  runId: string
  projectId: string
  request: PermissionRequest
}

const route = useRoute()
const router = useRouter()
const { t } = useI18n()
const live = useLiveRunsStore()
const projectsStore = useProjectsStore()
const runsStore = useRunsStore()
const tabsStore = useWorkspaceTabsStore()
const appSettings = useAppSettingsStore()

// The run currently open in RunView — its request is already shown there.
const activeRunId = computed(() =>
  typeof route.params.runId === 'string' ? route.params.runId : null,
)

const pending = computed<PendingRun[]>(() => {
  const out: PendingRun[] = []
  live.sessions.forEach((s) => {
    const req = s.permissionQueue[0]
    if (!req) return
    if (s.runId === activeRunId.value) return
    out.push({ runId: s.runId, projectId: s.projectId, request: req })
  })
  return out
})

function projectName(projectId: string): string {
  return projectsStore.projects.find((p) => p.id === projectId)?.name ?? t('runNotify.unknownProject')
}

// Read at notify time, so a switch flipped in Settings applies to the next one.
function allowed(kind: RunNotificationKind): boolean {
  return allowsRunNotification(parseDesktopNotify(appSettings.settings?.desktop_notify), kind)
}

// The run's metadata (engine, title, conductor). A freshly-spawned run may not
// be in `runMeta` yet — `list_runs` is the only source of `conductor_run_id`.
async function runMetaFor(runId: string, projectId: string) {
  let meta = runsStore.runMeta.get(runId)
  if (!meta) {
    await runsStore.refreshMeta(projectId).catch(() => {})
    meta = runsStore.runMeta.get(runId)
  }
  return meta
}

function navigateToRun(projectId: string, runId: string) {
  // Register/open the project's workspace tab BEFORE routing — the run
  // workspace is keyed by projectId and driven by the tabs store, so a bare
  // router.push lands on an empty view when the tab isn't open yet.
  tabsStore.open(projectId, runId)
  router
    .push({ name: 'project-run-detail', params: { projectId, runId } })
    .catch(() => {})
}

// --- Native OS notifications ---------------------------------------------

let permissionGranted = false
let unlistenClick: UnlistenFn | null = null
let unlistenScheduleFailed: UnlistenFn | null = null
let unlistenScheduleClick: UnlistenFn | null = null
let unsubscribeDone: (() => void) | null = null
let unsubscribeReminder: (() => void) | null = null
const notified = new Set<string>()
let stuckNotified = new Set<string>()

// Native "session finished" notification. Skipped when the window is focused
// (the Active runs dock already shows it), for runs the user stopped (a quota
// stop, `endReason`, still notifies), and for conductor workers (only the main
// run). A turn a limit stopped (AC-23, `reason`) says why instead of the
// session title.
async function notifyRunFinished(
  runId: string,
  projectId: string,
  status: string,
  reason?: string,
  endReason?: string,
) {
  const kind = finishedRunKind(status, endReason)
  if (!permissionGranted || !kind || !allowed(kind)) return
  try {
    // Any app window counts (a popped-out session window, …).
    const wins = await getAllWebviewWindows()
    const focused = await Promise.all(wins.map((w) => w.isFocused().catch(() => false)))
    if (focused.some(Boolean)) return
  } catch {
    // Can't tell — fall through and notify.
  }
  const meta = await runMetaFor(runId, projectId)
  if (meta?.conductor_run_id) return
  const text = runNotificationText({
    kind,
    engine: meta?.engine,
    project: projectName(projectId),
    title: meta?.title,
    reason,
    t,
  })
  invoke('show_run_finished_notification', {
    ...text,
    projectId,
    runId,
    openLabel: t('runNotify.open'),
  }).catch(() => {
    // ignore — the Active runs dock still shows it in-app
  })
}

// "Needs you" notification for a waiting permission / question. The approval
// window and the phone push do not depend on it.
async function notifyNeedsYou(projectId: string, runId: string, req: PermissionRequest, reminder: boolean) {
  const meta = await runMetaFor(runId, projectId)
  const text = runNotificationText({
    kind: 'needs_you',
    engine: meta?.engine,
    project: projectName(projectId),
    request: req,
    reminder,
    t,
  })
  // Fire via the native Rust command so notification clicks route back to
  // this run (the plugin's sendNotification has no working desktop click).
  invoke('show_permission_notification', {
    ...text,
    projectId,
    runId,
    openLabel: t('runNotify.open'),
  }).catch(() => {
    // ignore — the History attention icon and the in-app banner still cover it
  })
}

// The backend watchdog says a prompt has been waiting a long time: re-fire the
// native notification (unless the app is in front of the user — the in-app
// banner and window already show it). The prompt itself is never auto-answered.
async function remindPermission(projectId: string, runId: string, req: PermissionRequest) {
  if (!permissionGranted || !allowed('needs_you')) return
  try {
    const wins = await getAllWebviewWindows()
    const focused = await Promise.all(wins.map((w) => w.isFocused().catch(() => false)))
    if (focused.some(Boolean)) return
  } catch {
    // Can't tell — fall through and notify.
  }
  await notifyNeedsYou(projectId, runId, req, true)
}

// "Session may be stuck" notification for one watchdog alert. Unlike a finished
// turn, a conductor worker is not skipped.
async function notifyStuck(alert: StuckAlert) {
  try {
    const wins = await getAllWebviewWindows()
    const focused = await Promise.all(wins.map((w) => w.isFocused().catch(() => false)))
    if (focused.some(Boolean)) return
  } catch {
    // Can't tell — fall through and notify.
  }
  const meta = await runMetaFor(alert.runId, alert.projectId)
  const text = runNotificationText({
    kind: 'stuck',
    engine: meta?.engine,
    project: projectName(alert.projectId),
    alert: alert.kind,
    t,
  })
  invoke('show_run_finished_notification', {
    ...text,
    projectId: alert.projectId,
    runId: alert.runId,
    openLabel: t('runNotify.open'),
  }).catch(() => {
    // ignore — the session banner and the Active runs dock mark still show it
  })
}

// "Schedule did not start" notification. Sent focused or not: a schedule runs
// while nobody is around, and no window shows this failure.
function notifyScheduleFailed(name: string, message: string) {
  if (!permissionGranted || !allowed('schedule')) return
  const text = runNotificationText({ kind: 'schedule', project: '', schedule: name, reason: message, t })
  invoke('show_schedule_failed_notification', { ...text, openLabel: t('runNotify.open') }).catch(() => {
    // ignore — the Schedules screen still shows the error on the schedule's row
  })
}

onMounted(async () => {
  try {
    permissionGranted = await isPermissionGranted()
    if (!permissionGranted) {
      permissionGranted = (await requestPermission()) === 'granted'
    }
  } catch {
    permissionGranted = false
  }

  unsubscribeDone = live.onAnyDone((runId, projectId, status, reason, endReason) => {
    void notifyRunFinished(runId, projectId, status, reason, endReason)
  })
  unsubscribeReminder = live.onPermissionReminder((runId, projectId, req) => {
    void remindPermission(projectId, runId, req)
  })

  // Click handling is done natively (see src-tauri notifications.rs): the plugin's
  // `onAction` never fires on desktop, so the Rust side emits this event carrying
  // the run identity when the user clicks the notification.
  try {
    unlistenClick = await listen<{ projectId?: string; runId?: string }>(
      'permission-notification-clicked',
      (e) => {
        const { projectId, runId } = e.payload ?? {}
        getCurrentWindow()
          .setFocus()
          .catch(() => {})
        if (projectId && runId) {
          navigateToRun(projectId, runId)
        }
      },
    )
  } catch {
    unlistenClick = null
  }

  // A remote-desktop window drives another machine, whose own window notifies.
  if (isRemoteMode()) return
  try {
    unlistenScheduleFailed = await listen<{ name?: string; message?: string }>(EVT_SCHEDULE_FAILED, (e) => {
      notifyScheduleFailed(e.payload?.name ?? '', e.payload?.message ?? '')
    })
    unlistenScheduleClick = await listen('schedule-failed-notification-clicked', () => {
      getCurrentWindow()
        .setFocus()
        .catch(() => {})
      router.push({ name: 'schedules' }).catch(() => {})
    })
  } catch {
    // Can't listen — the Schedules screen still shows the error.
  }
})

onBeforeUnmount(() => {
  unlistenClick?.()
  unlistenClick = null
  unlistenScheduleFailed?.()
  unlistenScheduleFailed = null
  unlistenScheduleClick?.()
  unlistenScheduleClick = null
  unsubscribeDone?.()
  unsubscribeDone = null
  unsubscribeReminder?.()
  unsubscribeReminder = null
})

// Fire a native notification once per new pending request (not on the run the
// user is already viewing). Prune ids that have left every queue so the same
// request can re-alert only if it genuinely reappears. A request that arrives
// while "Needs you" is off is still marked, so turning it back on does not
// replay it.
watch(
  pending,
  (list) => {
    const allPendingIds = new Set<string>()
    live.sessions.forEach((s) => {
      for (const r of s.permissionQueue) allPendingIds.add(r.request_id)
    })
    for (const id of [...notified]) {
      if (!allPendingIds.has(id)) notified.delete(id)
    }

    if (!permissionGranted) return
    const on = allowed('needs_you')
    for (const p of list) {
      const id = p.request.request_id
      if (notified.has(id)) continue
      notified.add(id)
      if (on) void notifyNeedsYou(p.projectId, p.runId, p.request, false)
    }
  },
  { deep: true },
)

const attentionByRun = computed<RunAttentionState[]>(() => {
  const out: RunAttentionState[] = []
  live.sessions.forEach((s) => out.push({ runId: s.runId, projectId: s.projectId, attention: s.attention }))
  return out
})

// One notification per stuck alert. An alert raised while the window is
// focused or "May be stuck" is off is still marked, so it never fires later.
watch(attentionByRun, (runs) => {
  const { fresh, notified: next } = newStuckAlerts(runs, stuckNotified)
  stuckNotified = next
  if (!fresh.length || !permissionGranted || !allowed('stuck')) return
  for (const alert of fresh) void notifyStuck(alert)
})
</script>

<template>
  <!-- Headless: the in-app signal is the animated icon in RunView's History list. -->
  <span hidden aria-hidden="true" />
</template>
