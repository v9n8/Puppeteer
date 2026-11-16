// The result card a conductor's conversation shows for each worker that finished a
// turn (V3-30). The backend writes one `_puppeteer_worker_results` record per nudge
// (src-tauri/src/conductor/worker_results.rs); this file turns a worker's numbers
// into the card's sub line. Kept free of Vue / DOM / `@/` imports so it can be
// unit-tested with `node --test`.

import { formatTokensShort } from './contextLimits.ts'

export const WORKER_RESULTS_TYPE = '_puppeteer_worker_results'

export interface WorkerResult {
  id: string
  title: string
  status: string
  finished_at: string
  duration_ms?: number
  total_tokens?: number
}

/** What the worker's edit records add up to (the Changed files tab's own numbers). */
export interface WorkerEdits {
  files: number
  added: number
  removed: number
}

type Translate = (key: string, params?: Record<string, unknown>) => string

/** The 8-character id the worker column shows. */
export function shortWorkerId(id: string): string {
  return id.slice(0, 8)
}

/** "42s", "6m 12s", "1h 5m", from milliseconds. Empty when the value is not a usable duration. */
export function formatDuration(ms: number | undefined, t: Translate): string {
  if (typeof ms !== 'number' || !Number.isFinite(ms) || ms <= 0) return ''
  const total = Math.max(1, Math.round(ms / 1000))
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  if (h > 0) return t('conductor.result.durHourMin', { h, m })
  if (m > 0) return t('conductor.result.durMinSec', { m, s })
  return t('conductor.result.durSec', { s })
}

/**
 * "Edited 3 files · +120 −4 · 6m 12s · 41k tokens". A part with no data is left out
 * rather than written as 0: `edits` is null until the records load or when the worker
 * edited nothing, and a missing duration / token count is simply absent.
 */
export function workerResultLine(w: WorkerResult, edits: WorkerEdits | null, t: Translate): string {
  const parts: string[] = []
  if (edits && edits.files > 0) {
    parts.push(t('conductor.result.edited', { n: edits.files }))
    parts.push(`+${edits.added} −${edits.removed}`)
  }
  const dur = formatDuration(w.duration_ms, t)
  if (dur) parts.push(dur)
  if (typeof w.total_tokens === 'number' && w.total_tokens > 0) {
    parts.push(t('conductor.result.tokens', { n: formatTokensShort(w.total_tokens) }))
  }
  return parts.join(' · ')
}

/** Read the record's `workers`, dropping anything that is not a worker row. */
export function parseWorkerResults(e: Record<string, unknown>): WorkerResult[] {
  if (!Array.isArray(e.workers)) return []
  const out: WorkerResult[] = []
  for (const raw of e.workers) {
    if (!raw || typeof raw !== 'object') continue
    const w = raw as Record<string, unknown>
    if (typeof w.id !== 'string' || !w.id) continue
    out.push({
      id: w.id,
      title: typeof w.title === 'string' ? w.title : '',
      status: typeof w.status === 'string' ? w.status : 'done',
      finished_at: typeof w.finished_at === 'string' ? w.finished_at : '',
      ...(typeof w.duration_ms === 'number' ? { duration_ms: w.duration_ms } : {}),
      ...(typeof w.total_tokens === 'number' ? { total_tokens: w.total_tokens } : {}),
    })
  }
  return out
}

/**
 * A conductor nudge right after a card record is the cards' own message: it moves onto
 * them (`nudge`) instead of becoming an entry. Returns true when it was taken.
 */
export function attachNudge(
  entries: { kind: string; nudge?: string }[],
  injected: { source?: string; text: string },
): boolean {
  const last = entries[entries.length - 1]
  if (injected.source !== 'session' || last?.kind !== 'worker_results') return false
  entries[entries.length - 1] = { ...last, nudge: injected.text }
  return true
}
