// Desktop OS notifications about runs (HD-09): which kind a finished turn
// raises, which `desktop_notify` switch governs it, and the translated title /
// body. PermissionNotifier decides when to notify; this only names things.

import { allows, type DesktopNotify, type DesktopNotifyKind } from './desktopNotify.ts'
import { engineLabel } from './skillRows.ts'

/** HD-11 adds 'stuck', HD-12 'schedule'. */
export type RunNotificationKind = 'needs_you' | 'done' | 'failed' | 'stopped_quota'

/** `runs.end_reason` of a session the quota stop ended (backend `quota_stop::END_REASON`). */
export const QUOTA_END_REASON = 'quota'

export type Translate = (key: string, params?: Record<string, unknown>) => string

/**
 * The notification a finished turn raises, or null. A session the user
 * stopped raises none; one the quota stop ended is told apart by its
 * `end_reason`.
 */
export function finishedRunKind(status: string, endReason?: string | null): RunNotificationKind | null {
  if (status === 'done') return 'done'
  if (status === 'failed') return 'failed'
  if (status === 'cancelled' && endReason === QUOTA_END_REASON) return 'stopped_quota'
  return null
}

/** The switch that governs `kind`: a quota stop counts as "Failed or stopped". */
export function notifySwitch(kind: RunNotificationKind): DesktopNotifyKind {
  return kind === 'stopped_quota' ? 'failed' : kind
}

/** Whether the user's switches let `kind` notify. */
export function allowsRunNotification(settings: DesktopNotify, kind: RunNotificationKind): boolean {
  return allows(settings, notifySwitch(kind))
}

/** The parts of a permission request the text reads. */
export interface NotificationRequest {
  tool_name: string
  title?: string | null
  display_name?: string | null
}

export interface RunNotificationInput {
  kind: RunNotificationKind
  /** The run's engine id ('claude' | 'codex'); unknown → no engine named. */
  engine?: string | null
  project: string
  /** The session title, shown as the body of a finished turn. */
  title?: string | null
  /** Why a limit stopped the turn (`run:done.reason`, from the backend). */
  reason?: string | null
  /** needs_you: the waiting request. */
  request?: NotificationRequest | null
  /** needs_you: a reminder for a request that has waited a long time. */
  reminder?: boolean
  t: Translate
}

function engineName(engine: string | null | undefined, t: Translate): string {
  return engine?.trim() ? engineLabel(engine.trim()) : t('runNotify.someEngine')
}

function requestText(req: NotificationRequest | null | undefined, engine: string, t: Translate): string {
  if (!req) return ''
  if (req.tool_name === 'AskUserQuestion') return t('runNotify.body.question', { engine })
  if (req.tool_name === 'ExitPlanMode') return t('runNotify.body.planReady', { engine })
  // The bridge's own sentence for the request, when it sent one.
  return req.title || req.display_name || t('runNotify.body.wantsToRun', { tool: req.tool_name })
}

/** Title and body of a run notification, in the app's language. */
export function runNotificationText(input: RunNotificationInput): { title: string; body: string } {
  const { kind, project, t } = input
  const engine = engineName(input.engine, t)
  const sessionTitle = input.title?.trim() || ''
  switch (kind) {
    case 'needs_you':
      return {
        title: t(input.reminder ? 'runNotify.title.stillWaiting' : 'runNotify.title.needsYou', { project }),
        body: requestText(input.request, engine, t),
      }
    case 'stopped_quota':
      // No %: the figure stays in the session's log.
      return { title: t('runNotify.title.stopped', { project }), body: t('runNotify.body.stoppedQuota') }
    case 'done':
    case 'failed': {
      const reason = input.reason?.trim()
      if (reason) return { title: t('runNotify.title.stopped', { project }), body: reason }
      return {
        title: t(kind === 'done' ? 'runNotify.title.done' : 'runNotify.title.failed', { project }),
        body: sessionTitle || t(kind === 'done' ? 'runNotify.body.done' : 'runNotify.body.failed', { engine }),
      }
    }
  }
}
