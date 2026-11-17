// Split the composer text into plain runs and `@path` mentions, so the overlay
// behind the textarea (VF-13) can draw a chip under each mention that names a
// real project file. The pieces join back to exactly the input: the overlay must
// lay out character for character like the textarea it sits under.
import { TRAILING_PUNCT } from './fileLinks.ts'

export interface MentionSegment {
  text: string
  mention: boolean
}

// Past this the overlay would double a very long paste in the DOM for little use.
export const MENTION_OVERLAY_MAX_CHARS = 20000

// Same rule as the `@` popup (useRunComposer detectMention): the `@` starts the
// text or follows whitespace, and the token runs to the next whitespace.
const MENTION_RE = /(?<=^|\s)@(\S+)/g

export function splitMentions(text: string, resolve: (raw: string) => string | null): MentionSegment[] {
  if (!text) return []
  if (text.length > MENTION_OVERLAY_MAX_CHARS || !text.includes('@')) return [{ text, mention: false }]
  const out: MentionSegment[] = []
  let last = 0
  for (const m of text.matchAll(MENTION_RE)) {
    // Punctuation closing the sentence stays outside the chip.
    const token = m[1].replace(TRAILING_PUNCT, '')
    if (!token || resolve(token) === null) continue
    const start = m.index
    const end = start + 1 + token.length
    if (start > last) out.push({ text: text.slice(last, start), mention: false })
    out.push({ text: text.slice(start, end), mention: true })
    last = end
  }
  if (last < text.length) out.push({ text: text.slice(last), mention: false })
  return out
}
