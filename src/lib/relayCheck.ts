// Settings → Devices "Check relay": turns the backend's `RelayCheck`
// (src-tauri/src/remote/relay_check.rs) into the one line shown under the button.
// It says only what the check saw.

export interface RelayCheck {
  reachable: boolean
  connect_ms: number | null
  token: 'accepted' | 'rejected' | 'no_reply' | 'not_checked'
  error_code: string | null
  error: string | null
  reply_wait_secs: number
}

type Translate = (key: string, params?: Record<string, unknown>) => string

export function relayCheckLine(r: RelayCheck, t: Translate): string {
  const k = (name: string) => `remote.settings.relayCheck.${name}`
  if (!r.reachable) return t(k('failed'), { error: r.error ?? '' })
  const ms = r.connect_ms ?? 0
  switch (r.token) {
    case 'accepted':
      return t(k('accepted'), { ms })
    case 'rejected':
      return t(k('rejected'), { ms, code: r.error_code ?? '?' })
    case 'no_reply':
      return r.error
        ? t(k('noReplyWhy'), { ms, error: r.error })
        : t(k('noReply'), { ms, s: r.reply_wait_secs })
    default:
      return t(k('notChecked'), { ms })
  }
}
