// What the Settings › About "Updates" card says and offers, as pure functions
// (tested in __tests__/updateStatus.test.mjs). The state itself lives in
// composables/useUpdateCheck, shared with the corner toast.

export type UpdateStatus = 'idle' | 'checking' | 'up_to_date' | 'available' | 'unsupported' | 'error'

/** The `check_update` command's answer. */
export type UpdateCheckResult =
  | { status: 'unsupported' | 'up_to_date' }
  | { status: 'available'; version: string; current_version: string }

export interface UpdateState {
  status: UpdateStatus
  /** The release on offer; set while `status` is `available`. */
  version: string | null
  error: string | null
}

/** The state a finished check leaves: its answer, or the error it threw. */
export function updateStatusFrom(outcome: UpdateCheckResult | { error: unknown }): UpdateState {
  if ('error' in outcome) return { status: 'error', version: null, error: String(outcome.error) }
  if (outcome.status === 'available') return { status: 'available', version: outcome.version, error: null }
  return { status: outcome.status, version: null, error: null }
}

/** The button beside the line; `checking` and `installing` are the disabled forms. */
export type UpdateAction = 'none' | 'check' | 'checking' | 'retry' | 'install' | 'installing'

export interface UpdateLineInput extends UpdateState {
  /** `app.auto_update` is on. */
  enabled: boolean
  installing: boolean
  /** Epoch ms of the last finished check, or null before the first. */
  lastCheckedAt: number | null
}

type Translate = (key: string, params?: Record<string, string>) => string

export function updateLine(
  s: UpdateLineInput,
  t: Translate,
  formatTime: (ms: number) => string,
): { text: string; action: UpdateAction } {
  if (!s.enabled) return { text: t('update.card.disabled'), action: 'none' }
  const at = s.lastCheckedAt === null ? '' : formatTime(s.lastCheckedAt)
  switch (s.status) {
    case 'unsupported':
      return { text: t('update.card.unsupported'), action: 'none' }
    case 'checking':
      return { text: t('update.card.checking'), action: 'checking' }
    case 'up_to_date':
      return { text: t('update.card.upToDate', { time: at }), action: 'check' }
    case 'available':
      return { text: t('update.available', { version: s.version ?? '' }), action: s.installing ? 'installing' : 'install' }
    case 'error':
      return { text: t('update.card.error', { error: s.error ?? '' }), action: 'retry' }
    default:
      return {
        text: at ? t('update.card.idleLast', { time: at }) : t('update.card.idle'),
        action: 'check',
      }
  }
}
