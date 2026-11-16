// What file-editing tool calls change, as diffs and +added / −removed counts.
//
// One source for both places that show it (V3-26): the stat badge and diff of a
// single tool call in the conversation, and the Session's "Changed files" tab,
// which adds the calls up per file. Both go through `editParts` / `computeDiff`,
// so the numbers always match.

import { computeDiff } from './diff.ts'
import { codexEditParts, toolFileTargets, type DiffPart } from './toolFiles.ts'
import type { StreamEntry } from './streamEvents.ts'

// Extract the before/after pair(s) a file-editing tool changes: Edit → one pair,
// MultiEdit → one per edit, Write → whole content as an all-added block, Codex
// `fileChange` → one per patch hunk, tagged with the file it belongs to.
// Memoized per input object: the template re-reads this while rendering each
// part (to decide where a new per-file heading starts), and parsing a patch on
// every one of those reads would be quadratic for a large diff.
const editPartsCache = new WeakMap<object, DiffPart[]>()
export function editParts(name: string, input: unknown): DiffPart[] {
  if (!input || typeof input !== 'object') return []
  const cached = editPartsCache.get(input)
  if (cached) return cached
  const parts = computeEditParts(name, input)
  editPartsCache.set(input, parts)
  return parts
}

export function computeEditParts(name: string, input: unknown): DiffPart[] {
  if (!input || typeof input !== 'object') return []
  const obj = input as Record<string, unknown>
  const n = (name || '').toLowerCase()
  const str = (v: unknown): string => (typeof v === 'string' ? v : '')
  if (obj.changes) return codexEditParts(obj)
  if (n === 'edit') {
    const before = str(obj.old_string)
    const after = str(obj.new_string)
    return before || after ? [{ before, after }] : []
  }
  if (n === 'multiedit' && Array.isArray(obj.edits)) {
    return (obj.edits as unknown[])
      .map((e) => (e && typeof e === 'object' ? (e as Record<string, unknown>) : {}))
      .map((e) => ({ before: str(e.old_string), after: str(e.new_string) }))
      .filter((d) => d.before || d.after)
  }
  if (n === 'write') {
    const after = str(obj.content)
    return after ? [{ before: '', after }] : []
  }
  return []
}

// Aggregate +added / −removed across all diff parts for the header badge.
// Memoized per input object: the template reads editStats up to four times per
// entry and each call runs an O(n·m) LCS — computing it once per entry matters
// for MultiEdit calls with many parts.
export interface EditStats {
  added: number
  removed: number
}

function sumParts(parts: DiffPart[]): EditStats {
  return parts.reduce(
    (acc, p) => {
      const d = computeDiff(p.before, p.after, { fold: false })
      return { added: acc.added + d.added, removed: acc.removed + d.removed }
    },
    { added: 0, removed: 0 },
  )
}

const editStatsCache = new WeakMap<object, EditStats>()
export function editStats(name: string, input: unknown): EditStats {
  if (!input || typeof input !== 'object') return { added: 0, removed: 0 }
  const hit = editStatsCache.get(input)
  if (hit) return hit
  const stats = sumParts(editParts(name, input))
  editStatsCache.set(input, stats)
  return stats
}

/** One successful call that changed a file. */
export interface FileEdit {
  /** Which prompt of the Session it ran under, from 1. */
  turn: number
  parts: DiffPart[]
  /** Its text was too large to bring over (`get_run_edit_records` dropped it). */
  tooLarge: boolean
}

export interface ChangedFile {
  path: string
  /** `create` when the Session's first call on the file was a `Write`. */
  kind: 'edit' | 'create'
  added: number
  removed: number
  /** At least one edit was too large to count: show "+?". */
  unknown: boolean
  edits: FileEdit[]
}

/** Tools that write a file: Write, Edit, MultiEdit, NotebookEdit, Codex patches. */
export function isWriteTool(name: string): boolean {
  const n = (name || '').toLowerCase()
  return n === 'write' || n.includes('edit') || n.includes('patch')
}

/**
 * The files a Session changed, newest change first: every successful edit of a
 * file added up. A call that failed never reached the disk and is left out; one
 * with no result yet counts (it is still running). `tooLarge` holds the ids of
 * calls whose text could not be brought over.
 */
export function changedFiles(entries: StreamEntry[], tooLarge: ReadonlySet<string> = new Set()): ChangedFile[] {
  const byPath = new Map<string, { file: ChangedFile; last: number }>()
  let turn = 0
  entries.forEach((e, idx) => {
    if (e.kind === 'user') turn++
    if (e.kind !== 'tool' || !isWriteTool(e.name) || e.result?.is_error) return
    const targets = toolFileTargets(e.input)
    if (!targets.length) return
    const big = tooLarge.has(e.id)
    const all = big ? [] : editParts(e.name, e.input)
    for (const path of targets) {
      // A Codex call can span files: its parts say which is theirs. The rest
      // belong to the call's only file.
      const parts = all.filter((p) => (p.path ? p.path === path : path === targets[0]))
      let slot = byPath.get(path)
      if (!slot) {
        const created = (e.name || '').toLowerCase() === 'write'
        slot = { file: { path, kind: created ? 'create' : 'edit', added: 0, removed: 0, unknown: false, edits: [] }, last: idx }
        byPath.set(path, slot)
      }
      const s = sumParts(parts)
      slot.file.added += s.added
      slot.file.removed += s.removed
      if (big) slot.file.unknown = true
      slot.file.edits.push({ turn: Math.max(1, turn), parts, tooLarge: big })
      slot.last = idx
    }
  })
  return [...byPath.values()].sort((a, b) => b.last - a.last).map((s) => s.file)
}

/**
 * A scan of the log read from disk, brought up to date with the entries the
 * running Session holds: the log is written to disk in batches, so the newest
 * calls may be missing from the scan, or still without their result. A live
 * file-editing call replaces the scanned one with its id and is added when the
 * scan has none; the scan's user prompts keep numbering the turns.
 */
export function mergeLiveEdits(scanned: StreamEntry[], live: StreamEntry[]): StreamEntry[] {
  const liveTools = new Map<string, StreamEntry>()
  for (const e of live) if (e.kind === 'tool' && e.id && isWriteTool(e.name)) liveTools.set(e.id, e)
  if (!liveTools.size) return scanned
  const seen = new Set<string>()
  const merged = scanned.map((e) => {
    if (e.kind !== 'tool') return e
    seen.add(e.id)
    return liveTools.get(e.id) ?? e
  })
  for (const [id, e] of liveTools) if (!seen.has(id)) merged.push(e)
  return merged
}
