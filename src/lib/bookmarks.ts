// Bookmarks on the turns of a session (src-tauri/src/runs/bookmarks.rs). A
// bookmark is anchored by the byte offset of its turn's prompt record in the run
// log plus the prompt's label, and placed here on the session's prompt list —
// the whole-log scan RunView keeps for the prompt navigator, not the DOM, which
// only holds a window of the log. Kept free of Vue / Tauri / `@/` imports so it
// can be unit-tested with `node --test`.

export type BookmarkSide = 'question' | 'answer'

export interface RunBookmark {
  id: string
  run_id: string
  /** Byte offset of the turn's prompt record in the run log. */
  log_offset: number
  side: BookmarkSide
  label: string
  created_at: string
}

/** One prompt of the session, in order; `offset` known once the log was scanned. */
export interface SessionPrompt {
  label: string
  offset?: number
}

export interface PlacedBookmark {
  bookmark: RunBookmark
  /** Index of the bookmarked turn's prompt in the session. */
  seq: number
}

/**
 * Where each bookmark is among the session's prompts: the prompt at its offset
 * with its label, else the prompt with its label nearest to its offset (the log
 * was rebuilt from the engine transcript and offsets moved). A bookmark that
 * matches no prompt is left out: its turn is gone. Sorted by turn, question
 * before answer.
 */
export function placeBookmarks(bookmarks: readonly RunBookmark[], prompts: readonly SessionPrompt[]): PlacedBookmark[] {
  const placed: PlacedBookmark[] = []
  for (const bookmark of bookmarks) {
    let seq = prompts.findIndex((p) => p.offset === bookmark.log_offset && p.label === bookmark.label)
    if (seq < 0) {
      let best = Infinity
      prompts.forEach((p, i) => {
        if (p.offset === undefined || p.label !== bookmark.label) return
        const distance = Math.abs(p.offset - bookmark.log_offset)
        if (distance < best) {
          best = distance
          seq = i
        }
      })
    }
    if (seq >= 0) placed.push({ bookmark, seq })
  }
  return placed.sort((a, b) => a.seq - b.seq || (a.bookmark.side === 'question' ? -1 : 1))
}

/** Key of one side of one turn, for looking a row's bookmark up. */
export function bookmarkKey(seq: number, side: BookmarkSide): string {
  return `${seq}:${side}`
}
