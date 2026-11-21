// What a running worker is doing, for the conductor's worker panel (CO-06). The
// panel used to show only how long the worker had been running: its run row's
// `last_activity_at` is written when a turn starts, not while it runs. The
// worker's own `run:event:<id>` stream is already emitted to every window, so
// the panel follows it. Kept free of Vue / Tauri imports for `node --test`.

export interface WorkerActivity {
  /** When the worker last emitted any event (ms since epoch). */
  at: number
  /** The tool its main thread called last; null while it writes text. */
  tool: string | null
}

/** The tool named by a main-thread assistant message, null for one with no tool call. */
function mainToolName(e: Record<string, unknown>): string | null | undefined {
  // Sub-agent messages carry `parent_tool_use_id`; the main thread's own call
  // (e.g. `Agent`) is what the worker is doing.
  if (e.type !== 'assistant' || e.parent_tool_use_id != null) return undefined
  const content = (e.message as Record<string, unknown> | undefined)?.content
  if (!Array.isArray(content)) return undefined
  for (let i = content.length - 1; i >= 0; i--) {
    const b = content[i] as Record<string, unknown> | null
    if (b?.type === 'tool_use' && typeof b.name === 'string') return b.name
  }
  return null
}

/** The activity after one event. Any event moves `at`; only a main-thread message changes `tool`. */
export function nextActivity(prev: WorkerActivity | undefined, e: unknown, now: number): WorkerActivity {
  const tool = e && typeof e === 'object' ? mainToolName(e as Record<string, unknown>) : undefined
  return { at: now, tool: tool === undefined ? (prev?.tool ?? null) : tool }
}

/**
 * When a settled worker was last active: the later of `last_activity_at` (its last
 * turn's start) and `finished_at` (its end). `runActivityAt` prefers the first,
 * which is the turn's start, so a worker that ran an hour showed as an hour older.
 */
export function settledAt(run: {
  last_activity_at?: string | null
  finished_at?: string | null
  started_at?: string | null
  created_at: string
}): string {
  const a = run.last_activity_at
  const f = run.finished_at
  if (a && f) return new Date(f).getTime() > new Date(a).getTime() ? f : a
  return a ?? f ?? run.started_at ?? run.created_at
}
