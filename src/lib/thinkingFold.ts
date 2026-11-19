// Folding one thinking block inside an open turn (components/sessions/
// ThinkingBlock.vue): it shows its heading and first line until the reader opens
// it, a ⌘F query hits its text, or it is still being streamed. Kept free of Vue /
// DOM / `@/` imports so it can be unit-tested with `node --test`.

interface EntryLike {
  kind: string
  text?: string
}

/** The first non-blank line of the thinking, for the folded heading. */
export function thinkingPreview(text: string): string {
  for (const line of text.split('\n')) {
    const trimmed = line.trim()
    if (trimmed) return trimmed
  }
  return ''
}

/**
 * Whether a thinking block shows its full text. A choice the reader made wins;
 * otherwise it is open while streaming (and folds itself once done) or when a
 * ⌘F query hits it, so the in-page highlight can reach the match.
 */
export function thinkingOpen(opts: { choice?: boolean; streaming?: boolean; findHit?: boolean }): boolean {
  if (typeof opts.choice === 'boolean') return opts.choice
  return !!opts.streaming || !!opts.findHit
}

/** Indices of the thinking entries whose text holds the ⌘F query. */
export function thinkingFindHits(
  entries: readonly EntryLike[],
  query: string | undefined,
  caseSensitive = false,
): Set<number> {
  const hits = new Set<number>()
  if (!query) return hits
  const q = caseSensitive ? query : query.toLowerCase()
  entries.forEach((e, i) => {
    if (e.kind !== 'thinking' || !e.text) return
    if ((caseSensitive ? e.text : e.text.toLowerCase()).includes(q)) hits.add(i)
  })
  return hits
}
