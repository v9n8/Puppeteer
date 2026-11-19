// The worker cap of a new session (QO-03): how many workers its conductor may
// run at once. Mirrors `conductor::defaults` in the backend, which applies the
// same bounds and default when a launch names no cap.

export const MIN_WORKER_CAP = 1
export const MAX_WORKER_CAP = 100
/** The cap while the `default_max_workers` setting is unset or unreadable. */
export const DEFAULT_WORKER_CAP = 3

/**
 * The `default_max_workers` setting (or a typed value) as a cap: a whole number
 * kept within 1–100; blank, missing or not a whole number → the default.
 */
export function defaultWorkerCap(raw: string | number | null | undefined): number {
  const text = String(raw ?? '').trim()
  if (!/^-?\d+$/.test(text)) return DEFAULT_WORKER_CAP
  return Math.min(MAX_WORKER_CAP, Math.max(MIN_WORKER_CAP, Number(text)))
}
