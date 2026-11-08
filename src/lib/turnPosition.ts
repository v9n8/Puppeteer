// Where the reader sits among the user's prompts (composables/useTurnNavigator.ts,
// the `n/N` counter and "Top" button in views/RunView.vue). Kept free of Vue /
// DOM imports so it can be unit-tested with `node --test`.

/**
 * Index of the last turn whose top is at or above `anchor`, or -1 when every
 * turn is below it (the reader is above the first one). `topAt(i)` is turn i's
 * top relative to the scroll container's visible top edge — the turns are in
 * document order, so those tops never decrease. Binary search, so a caller
 * reading tops from the DOM touches O(log n) rects instead of every turn's.
 */
export function turnIndexAt(count: number, topAt: (i: number) => number, anchor: number): number {
  let lo = 0
  let hi = count - 1
  let found = -1
  while (lo <= hi) {
    const mid = (lo + hi) >> 1
    if (topAt(mid) <= anchor) {
      found = mid
      lo = mid + 1
    } else {
      hi = mid - 1
    }
  }
  return found
}

/** Navigator counter: `n/N` (1-based), or `–/N` above the first prompt. */
export function turnCounterLabel(seq: number, total: number): string {
  return `${seq < 0 ? '–' : seq + 1}/${total}`
}

/**
 * Whether the reader has left the start of the log, i.e. "Top" has somewhere
 * to go. True when the loaded window starts after the session's first prompt,
 * or when the first loaded prompt has scrolled past the top edge. Landing on
 * the first prompt (a small positive gap above it) counts as being at the top.
 */
export function isAwayFromTop(windowStartsLater: boolean, firstTurnTop: number | null): boolean {
  return windowStartsLater || (firstTurnTop !== null && firstTurnTop < 0)
}

/**
 * Session-prompt index of the first prompt in a window loaded from byte
 * `offset`: the number of prompts that start before it. A prompt whose offset
 * is unknown (not scanned yet) is not counted.
 */
export function promptsBefore(promptOffsets: readonly (number | undefined)[], offset: number): number {
  return promptOffsets.filter((o) => typeof o === 'number' && o < offset).length
}
