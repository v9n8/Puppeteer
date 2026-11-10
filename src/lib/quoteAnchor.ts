// Where a quote sits in the message it was taken from (UX-29), and back.
//
// A quote of an assistant message is anchored by the message's id plus two
// offsets into the text that message RENDERS as: the text nodes of its
// markdown block, in document order (`quoteTextNodes`). The offsets come from
// the selection's own DOM positions, never from searching for the excerpt, so
// a phrase that occurs twice, or a range running through **bold**, `code` or a
// link, maps back to exactly what was selected. AWOG searched every block for
// the excerpt and so highlighted the first occurrence, or nothing.
//
// The pure half works on the block's text pieces (one string per text node)
// and is unit-tested with `node --test`; the DOM half below only collects the
// pieces and turns offsets into a Range. No Vue / Tauri / `@/` imports.

/** Marks a message's rendered markdown block (StreamLog): the message id. */
export const QUOTE_SRC_ATTR = 'data-quote-src'
/** Which text block of that message it is (`StreamEntry.block`). */
export const QUOTE_BLOCK_ATTR = 'data-quote-block'

/** A point in a block: `offset` UTF-16 code units into text piece `piece`. */
export interface PiecePoint {
  piece: number
  offset: number
}

/** The offset of `point` in the block's whole text. */
export function textOffset(pieces: readonly string[], point: PiecePoint): number {
  let n = 0
  for (let i = 0; i < point.piece && i < pieces.length; i++) n += pieces[i].length
  const own = pieces[point.piece]?.length ?? 0
  return n + Math.max(0, Math.min(point.offset, own))
}

/**
 * The piece an offset of the whole text falls in. At the seam of two pieces a
 * range's start goes to the next piece and its end stays in the previous one,
 * so a highlight never starts or ends on an empty stretch. Null past the end.
 */
export function pieceAt(pieces: readonly string[], offset: number, edge: 'start' | 'end'): PiecePoint | null {
  if (offset < 0) return null
  let acc = 0
  for (let i = 0; i < pieces.length; i++) {
    const len = pieces[i].length
    const inside = edge === 'start' ? offset < acc + len : offset <= acc + len && len > 0
    if (inside && offset >= acc) return { piece: i, offset: offset - acc }
    acc += len
  }
  if (offset !== acc) return null
  // The very end of the text: after the last non-empty piece.
  for (let i = pieces.length - 1; i >= 0; i--) if (pieces[i].length) return { piece: i, offset: pieces[i].length }
  return null
}

const isHigh = (c: number) => c >= 0xd800 && c <= 0xdbff
const isLow = (c: number) => c >= 0xdc00 && c <= 0xdfff

/** Never split a surrogate pair (an emoji): a start moves before it, an end after it. */
export function snapToCodePoint(text: string, offset: number, edge: 'start' | 'end'): number {
  if (offset <= 0 || offset >= text.length) return Math.max(0, Math.min(offset, text.length))
  if (isHigh(text.charCodeAt(offset - 1)) && isLow(text.charCodeAt(offset))) {
    return edge === 'start' ? offset - 1 : offset + 1
  }
  return offset
}

/**
 * The quote's offsets from a selection's two points: ordered, whole code points,
 * and without the blank space a drag picks up at its edges (the excerpt is
 * trimmed too). Null when nothing but blank space is selected.
 */
export function selectionOffsets(
  pieces: readonly string[],
  from: PiecePoint,
  to: PiecePoint,
): { start: number; end: number } | null {
  const full = pieces.join('')
  let start = textOffset(pieces, from)
  let end = textOffset(pieces, to)
  if (end < start) [start, end] = [end, start]
  while (start < end && /\s/.test(full[start])) start++
  while (end > start && /\s/.test(full[end - 1])) end--
  start = snapToCodePoint(full, start, 'start')
  end = snapToCodePoint(full, end, 'end')
  return end > start ? { start, end } : null
}

const squash = (s: string) => s.replace(/\s+/g, '')

/**
 * Whether the text at the stored offsets still is the quote. Blank space is
 * ignored: the selection's text (what the excerpt is) and the DOM's text nodes
 * space paragraphs, list items and table cells differently. A selection that ran
 * past its message keeps only the part inside it, so the excerpt may go on
 * after the slice.
 */
export function sliceMatches(slice: string, excerpt: string): boolean {
  const s = squash(slice)
  return !!s && squash(excerpt).startsWith(s)
}

/**
 * Where to highlight a quote in its block's text: the stored offsets when the
 * text there still is the quote (exact, even when the excerpt occurs several
 * times). Only when they no longer fit (the block renders differently since)
 * is the excerpt looked for, and only a single occurrence is trusted. Null:
 * no highlight, the caller just brings the message into view.
 */
export function resolveQuoteRange(
  pieces: readonly string[],
  start: number,
  end: number,
  excerpt: string,
): { start: number; end: number } | null {
  const full = pieces.join('')
  if (start >= 0 && end > start && end <= full.length && sliceMatches(full.slice(start, end), excerpt)) {
    return { start, end }
  }
  const needle = excerpt.trim()
  if (!needle) return null
  const at = full.indexOf(needle)
  if (at < 0 || full.indexOf(needle, at + 1) >= 0) return null
  return { start: at, end: at + needle.length }
}

/** A selection's anchor in the session log (QuoteAnchorRef in lib/quote). */
export interface QuoteAnchor {
  entryId: string
  block: number
  start: number
  end: number
}

// ── DOM ──────────────────────────────────────────────────────────────────────

/**
 * Text the reader doesn't see as the message: injected buttons (copy code),
 * icons, a mermaid diagram (its source turns into an SVG some time after the
 * first paint, which would shift every offset after it).
 */
const SKIP_SELECTOR = 'button, svg, .mermaid-diagram, [data-quote-skip]'

/** A block's text nodes in document order: the pieces offsets count in. */
export function quoteTextNodes(host: Element): Text[] {
  const doc = host.ownerDocument
  const walker = doc.createTreeWalker(host, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      const skip = node.parentElement?.closest(SKIP_SELECTOR)
      return skip && host.contains(skip) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT
    },
  })
  const out: Text[] = []
  for (let n = walker.nextNode(); n; n = walker.nextNode()) out.push(n as Text)
  return out
}

/** A DOM boundary point as a point in `nodes` (an element boundary falls between pieces). */
function piecePointOf(nodes: Text[], container: Node, offset: number): PiecePoint {
  const i = nodes.indexOf(container as Text)
  if (i >= 0) return { piece: i, offset }
  const at = container.ownerDocument!.createRange()
  at.setStart(container, offset)
  // Pieces before the point count whole: it is the start of the first after it.
  for (let k = 0; k < nodes.length; k++) {
    if (at.comparePoint(nodes[k], 0) >= 0) return { piece: k, offset: 0 }
  }
  return { piece: nodes.length - 1, offset: nodes[nodes.length - 1]?.data.length ?? 0 }
}

function elementOf(node: Node): Element | null {
  return node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement
}

/**
 * The anchor of a selection made in a session log: the message block its start
 * is in, and offsets into that block's text. A selection that runs on into the
 * next message is cut at the end of the first. Null outside a message block.
 */
export function anchorFromRange(range: Range | null | undefined): QuoteAnchor | null {
  if (!range || range.collapsed) return null
  const host = elementOf(range.startContainer)?.closest(`[${QUOTE_SRC_ATTR}]`)
  const entryId = host?.getAttribute(QUOTE_SRC_ATTR)
  if (!host || !entryId) return null
  const nodes = quoteTextNodes(host)
  if (!nodes.length) return null
  const pieces = nodes.map((n) => n.data)
  const from = piecePointOf(nodes, range.startContainer, range.startOffset)
  const to = host.contains(range.endContainer)
    ? piecePointOf(nodes, range.endContainer, range.endOffset)
    : { piece: nodes.length - 1, offset: pieces[pieces.length - 1].length }
  const offsets = selectionOffsets(pieces, from, to)
  if (!offsets) return null
  return { entryId, block: Number(host.getAttribute(QUOTE_BLOCK_ATTR)) || 0, ...offsets }
}

/** A message's blocks on screen, the quoted block first. */
export function quoteHosts(root: ParentNode, entryId: string, block: number): Element[] {
  const all = Array.from(root.querySelectorAll(`[${QUOTE_SRC_ATTR}="${CSS.escape(entryId)}"]`))
  const own = all.filter((el) => Number(el.getAttribute(QUOTE_BLOCK_ATTR)) === block)
  return [...own, ...all.filter((el) => !own.includes(el))]
}

/** The DOM range of a quote in one block, checked against the excerpt; null if it isn't there. */
export function quoteRangeIn(host: Element, start: number, end: number, excerpt: string): Range | null {
  const nodes = quoteTextNodes(host)
  const pieces = nodes.map((n) => n.data)
  const r = resolveQuoteRange(pieces, start, end, excerpt)
  if (!r) return null
  const a = pieceAt(pieces, r.start, 'start')
  const b = pieceAt(pieces, r.end, 'end')
  if (!a || !b) return null
  const range = host.ownerDocument.createRange()
  range.setStart(nodes[a.piece], a.offset)
  range.setEnd(nodes[b.piece], b.offset)
  return range
}

/** `::highlight()` name, styled in assets/main.css. */
const HIGHLIGHT = 'quote-source'
const FLASH_CLASS = 'quote-source-flash'
const FLASH_MS = 2400
let flashTimer: ReturnType<typeof setTimeout> | null = null
let flashed: { host: Element; highlight: unknown } | null = null

function clearFlash() {
  if (flashTimer) clearTimeout(flashTimer)
  flashTimer = null
  if (!flashed) return
  flashed.host.classList.remove(FLASH_CLASS)
  if (typeof CSS !== 'undefined' && 'highlights' in CSS && CSS.highlights.get(HIGHLIGHT) === flashed.highlight) {
    CSS.highlights.delete(HIGHLIGHT)
  }
  flashed = null
}

/**
 * Bring a quote's source into view and mark it briefly: the exact range when
 * there is one (CSS Custom Highlight API), else the whole message block.
 */
export function flashQuoteSource(host: Element, range: Range | null) {
  clearFlash()
  let highlight: unknown = null
  if (range && typeof CSS !== 'undefined' && 'highlights' in CSS && typeof Highlight !== 'undefined') {
    const hl = new Highlight(range)
    CSS.highlights.set(HIGHLIGHT, hl)
    highlight = hl
  } else {
    host.classList.add(FLASH_CLASS)
  }
  flashed = { host, highlight }
  const target = range?.startContainer.parentElement ?? host
  target.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'smooth' })
  flashTimer = setTimeout(clearFlash, FLASH_MS)
}
