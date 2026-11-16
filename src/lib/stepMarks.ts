// Reading the backend's `_puppeteer_step_at` log records (src-tauri/src/runs/step_marks.rs):
// when a tool call started and when its result came back, stamped on the tool entry
// so the turn status bar can show how long each step took. The record is not an
// entry of its own. Kept free of Vue / DOM / `@/` imports so it can be unit-tested
// with `node --test`.

export const STEP_MARK_TYPE = '_puppeteer_step_at'

interface ToolEntryLike {
  kind: string
  startedAt?: number
  endedAt?: number
}

/**
 * Stamp the tool entries a mark names. A new object replaces the entry (the log's
 * v-memo keys on identity). An id with no entry (a mark for a message outside the
 * loaded window) is ignored, and so is a mark that would not change anything.
 */
export function applyStepMark(
  state: { entries: ToolEntryLike[]; toolIndex: Map<string, number> },
  e: Record<string, unknown>,
): void {
  const at = typeof e.at === 'number' && Number.isFinite(e.at) ? e.at : null
  const phase = e.phase === 'start' ? 'startedAt' : e.phase === 'end' ? 'endedAt' : null
  if (at === null || phase === null || !Array.isArray(e.ids)) return
  for (const id of e.ids) {
    const idx = typeof id === 'string' ? state.toolIndex.get(id) : undefined
    const entry = idx === undefined ? undefined : state.entries[idx]
    if (idx === undefined || entry?.kind !== 'tool' || entry[phase] === at) continue
    state.entries[idx] = { ...entry, [phase]: at }
  }
}
