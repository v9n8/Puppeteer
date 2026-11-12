// Desktop OS notification switches, one JSON setting (`desktop_notify`):
// `{"needs_you":true,"done":true,"failed":true,"stuck":true,"schedule":true,"sound":true}`.
// Everything is on until the user turns it off, so a profile without the key
// (or with a broken value) keeps notifying as before.

/** Setting key of the switches. Local-only with Remote Control (`remote::guard`). */
export const DESKTOP_NOTIFY_SETTING = 'desktop_notify'

/** Notification kinds, then `sound` (whether they play a sound). Names are final: later work reads them. */
export const DESKTOP_NOTIFY_KINDS = ['needs_you', 'done', 'failed', 'stuck', 'schedule', 'sound'] as const
export type DesktopNotifyKind = (typeof DESKTOP_NOTIFY_KINDS)[number]
export type DesktopNotify = Record<DesktopNotifyKind, boolean>

/** The switches stored in `raw`: a missing key, broken JSON or a missing field reads as on. */
export function parseDesktopNotify(raw: string | null | undefined): DesktopNotify {
  let parsed: unknown = null
  if (raw) {
    try {
      parsed = JSON.parse(raw)
    } catch {
      parsed = null
    }
  }
  const fields = parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? (parsed as Record<string, unknown>) : {}
  const out = {} as DesktopNotify
  // Only an explicit `false` turns a kind off.
  for (const k of DESKTOP_NOTIFY_KINDS) out[k] = fields[k] !== false
  return out
}

/** Whether `kind` may notify (or, for `sound`, play a sound). */
export function allows(settings: DesktopNotify, kind: DesktopNotifyKind): boolean {
  return settings[kind] !== false
}
