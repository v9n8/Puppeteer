// RT-07: paging for the live pane (`components/sessions/useLivePaging.ts`).
// A live session holds what this window received since it started listening.
// When that is not the whole log — the copy was seeded from a history window
// on resume, or the window started listening mid-run (a prompt sent from the
// session pop-out, a run started elsewhere) — the older part is read from the
// log on disk, a page at a time, like the history pane. Kept free of Vue so it
// runs under `node --test`.

import type { StreamEntry } from './streamEvents'

/**
 * Where the live copy starts in the log. `cursor`: byte offset of its oldest
 * record loaded from disk (a `getRunLogPage` `before`); `hasMore`: older
 * records remain above it; `seedModel`: the model of the log's preamble, for
 * parsing pages that cannot see `system.init`.
 */
export interface LivePaging {
  cursor: number
  hasMore: boolean
  seedModel: string | null
}

/** A live copy that holds the log from its first record (a run started here). */
export const COMPLETE_PAGING: LivePaging = Object.freeze({ cursor: 0, hasMore: false, seedModel: null })

/**
 * Paging for a live copy seeded from the history window on screen. Null when
 * the window does not reach the end of the log (after a jump): the seed would
 * leave a hole, so the copy is rebuilt from the disk tail instead.
 */
export function pagingFromHistory(h: {
  cursor: number | null
  hasMore: boolean
  hasMoreBelow: boolean
  seedModel: string | null
}): LivePaging | null {
  if (h.hasMoreBelow) return null
  return { cursor: h.cursor ?? 0, hasMore: h.hasMore && h.cursor !== null, seedModel: h.seedModel }
}

/**
 * What identifies an entry in both the disk log and the live stream: a prompt
 * by its text, a tool call by its id. Text and thinking blocks are not used:
 * a live copy that started listening mid-block holds only a fragment.
 */
function anchorKey(e: StreamEntry): string | null {
  if (e.kind === 'user') return `u\u0000${e.text.trim()}`
  if (e.kind === 'tool' && e.id) return `t\u0000${e.id}`
  return null
}

function anchors(entries: StreamEntry[]): { idx: number; key: string }[] {
  const out: { idx: number; key: string }[] = []
  entries.forEach((e, idx) => {
    const key = anchorKey(e)
    if (key !== null) out.push({ idx, key })
  })
  return out
}

export type TailMerge = { kind: 'merged'; entries: StreamEntry[] } | { kind: 'older' }

/**
 * Join a page read from the end of the log (`disk`) with the live copy. The
 * two overlap where the copy began: the copy's first anchor is found in the
 * page (the latest place where every later anchor of the page lines up with
 * the copy), and the page supplies everything before it. The live entries are
 * kept from there on (they may still be streaming or waiting for an echo).
 *
 * `older`: the page lies inside the copy — the copy starts earlier, read the
 * page before it. When the page ends before the copy starts (it was read
 * before the drain flushed what the copy holds) both are kept, page first.
 */
export function mergeTailWithLive(disk: StreamEntry[], live: StreamEntry[]): TailMerge {
  const da = anchors(disk)
  const la = anchors(live)
  if (!disk.length) return { kind: 'merged', entries: [...live] }
  if (!la.length) return { kind: 'merged', entries: [...disk, ...live] }
  for (let p = da.length - 1; p >= 0; p--) {
    if (da[p].key !== la[0].key) continue
    const overlap = da.length - p
    if (overlap > la.length) continue
    let aligned = true
    for (let q = 1; q < overlap; q++) {
      if (da[p + q].key !== la[q].key) {
        aligned = false
        break
      }
    }
    if (aligned) return { kind: 'merged', entries: [...disk.slice(0, da[p].idx), ...live.slice(la[0].idx)] }
  }
  const lastDisk = da[da.length - 1]
  if (lastDisk && la.some((a) => a.key === lastDisk.key)) return { kind: 'older' }
  return { kind: 'merged', entries: [...disk, ...live] }
}

/** An older page in front of the entries already shown. */
export function prependPage(entries: StreamEntry[], older: StreamEntry[]): StreamEntry[] {
  return older.length ? [...older, ...entries] : entries
}

/** Tool-call id → entry index, as `applyStreamEvent` keeps it, for a rebuilt list. */
export function toolIndexOf(entries: StreamEntry[]): Map<string, number> {
  const index = new Map<string, number>()
  entries.forEach((e, i) => {
    if (e.kind === 'tool' && e.id) index.set(e.id, i)
  })
  return index
}
