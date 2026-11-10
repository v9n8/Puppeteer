// Unit tests for lib/quoteAnchor.ts: a quote's offsets ↔ the text its message
// renders as (UX-29). A rendered block is modelled as its text nodes in
// document order ("pieces"), which is what the DOM half collects.
// Run: node --test src/lib/__tests__/quoteAnchor.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  pieceAt, resolveQuoteRange, selectionOffsets, sliceMatches, snapToCodePoint, textOffset,
} from '../quoteAnchor.ts'

// `The **cat** sat on the `cat`, then [cat](https://x) left.` renders as these
// text nodes: plain, <strong>, plain, <code>, plain, <a>, plain.
const inline = ['The ', 'cat', ' sat on the ', 'cat', ', then ', 'cat', ' left.']
const joined = inline.join('')

/** Map a selection back to pieces, as the highlight does, and read the text there. */
function roundTrip(pieces, start, end) {
  const a = pieceAt(pieces, start, 'start')
  const b = pieceAt(pieces, end, 'end')
  let out = ''
  for (let i = a.piece; i <= b.piece; i++) {
    const from = i === a.piece ? a.offset : 0
    const to = i === b.piece ? b.offset : pieces[i].length
    out += pieces[i].slice(from, to)
  }
  return { a, b, text: out }
}

test('a repeated word: the selected occurrence is anchored, not the first', () => {
  // The user selects the `cat` inside <code> (piece 3).
  const o = selectionOffsets(inline, { piece: 3, offset: 0 }, { piece: 3, offset: 3 })
  assert.deepEqual(o, { start: 19, end: 22 })
  assert.notEqual(o.start, joined.indexOf('cat'), 'a text search would have found the bold one')
  // Back: the same text node, the same characters.
  const back = roundTrip(inline, o.start, o.end)
  assert.deepEqual([back.a, back.b], [{ piece: 3, offset: 0 }, { piece: 3, offset: 3 }])
  // The excerpt checks out at the stored offsets, so they are kept as they are.
  assert.deepEqual(resolveQuoteRange(inline, o.start, o.end, 'cat'), { start: 19, end: 22 })
})

test('the third occurrence (a link) round-trips too', () => {
  const o = selectionOffsets(inline, { piece: 5, offset: 0 }, { piece: 5, offset: 3 })
  assert.equal(joined.slice(o.start, o.end), 'cat')
  assert.equal(roundTrip(inline, o.start, o.end).a.piece, 5)
})

test('a range across bold, code and a link maps back to exactly that range', () => {
  // From "cat" (bold) to the end of the link text.
  const o = selectionOffsets(inline, { piece: 1, offset: 0 }, { piece: 5, offset: 3 })
  assert.equal(joined.slice(o.start, o.end), 'cat sat on the cat, then cat')
  const back = roundTrip(inline, o.start, o.end)
  assert.deepEqual([back.a, back.b], [{ piece: 1, offset: 0 }, { piece: 5, offset: 3 }])
  assert.equal(back.text, 'cat sat on the cat, then cat')
})

test('a selection made backwards (focus before anchor) is ordered', () => {
  assert.deepEqual(
    selectionOffsets(inline, { piece: 3, offset: 3 }, { piece: 3, offset: 0 }),
    selectionOffsets(inline, { piece: 3, offset: 0 }, { piece: 3, offset: 3 }),
  )
})

test('blank space a drag picks up at its edges is left out; only blanks → nothing', () => {
  const pieces = ['Hello', '\n', 'world', '\n']
  assert.deepEqual(selectionOffsets(pieces, { piece: 0, offset: 0 }, { piece: 3, offset: 1 }), { start: 0, end: 11 })
  assert.equal(selectionOffsets(pieces, { piece: 1, offset: 0 }, { piece: 1, offset: 1 }), null)
})

test('multi-line: a range over two paragraphs and a list keeps its newlines', () => {
  // <p>First line.</p>\n<p>Second</p>\n<ul>\n<li>one</li>\n<li>two</li>\n</ul>
  const pieces = ['First line.', '\n', 'Second', '\n', '\n', 'one', '\n', 'two', '\n']
  const o = selectionOffsets(pieces, { piece: 0, offset: 6 }, { piece: 7, offset: 3 })
  assert.equal(pieces.join('').slice(o.start, o.end), 'line.\nSecond\n\none\ntwo')
  // The selection's own text spaces blocks differently; whitespace aside it matches.
  assert.deepEqual(resolveQuoteRange(pieces, o.start, o.end, 'line.\nSecond\none\ntwo'), o)
})

test('pieceAt: at a seam a start goes forward, an end stays back', () => {
  assert.deepEqual(pieceAt(inline, 4, 'start'), { piece: 1, offset: 0 })
  assert.deepEqual(pieceAt(inline, 4, 'end'), { piece: 0, offset: 4 })
  assert.deepEqual(pieceAt(inline, joined.length, 'end'), { piece: 6, offset: 6 })
  assert.equal(pieceAt(inline, joined.length + 1, 'end'), null)
})

test('pieceAt skips empty text nodes', () => {
  const pieces = ['ab', '', 'cd']
  assert.deepEqual(pieceAt(pieces, 2, 'start'), { piece: 2, offset: 0 })
  assert.deepEqual(pieceAt(pieces, 2, 'end'), { piece: 0, offset: 2 })
})

test('textOffset clamps an offset past its piece', () => {
  assert.equal(textOffset(inline, { piece: 1, offset: 99 }), 7)
  assert.equal(textOffset(inline, { piece: 0, offset: 0 }), 0)
})

test('emoji: offsets are UTF-16 and never split a surrogate pair', () => {
  const pieces = ['Ship it 👍 now 🎉', ' done']
  const text = pieces.join('')
  const thumb = text.indexOf('👍')
  assert.equal('👍'.length, 2)
  // A start inside the pair moves before it, an end inside it moves after it.
  assert.equal(snapToCodePoint(text, thumb + 1, 'start'), thumb)
  assert.equal(snapToCodePoint(text, thumb + 1, 'end'), thumb + 2)
  // Selecting the emoji and the word after it.
  const o = selectionOffsets(pieces, { piece: 0, offset: thumb + 1 }, { piece: 0, offset: thumb + 6 })
  assert.equal(text.slice(o.start, o.end), '👍 now')
  // An emoji in a later text node: offsets count code units of the ones before.
  const party = text.indexOf('🎉')
  const back = roundTrip(pieces, party, pieces[0].length + 5)
  assert.equal(back.text, '🎉 done')
})

test('resolveQuoteRange: offsets that no longer fit fall back to a single occurrence only', () => {
  // The block now renders with a word more at the front: offsets point elsewhere.
  const now = ['Note: ', ...inline]
  assert.equal(resolveQuoteRange(now, 4, 7, 'cat'), null, '"cat" occurs three times: no guessing')
  assert.deepEqual(resolveQuoteRange(now, 0, 3, 'sat on'), { start: 14, end: 20 })
  assert.equal(resolveQuoteRange(now, 0, 3, 'not there'), null)
})

test('resolveQuoteRange: a selection that ran past its message keeps the part inside it', () => {
  const pieces = ['Use a queue here.']
  // The excerpt runs on into the next message; the anchor stops at this one's end.
  assert.deepEqual(resolveQuoteRange(pieces, 6, 17, 'queue here.\nAnd then'), { start: 6, end: 17 })
})

test('sliceMatches ignores spacing, not content', () => {
  assert.equal(sliceMatches('a\nb', 'a b'), true)
  assert.equal(sliceMatches('cell1cell2', 'cell1\tcell2'), true)
  assert.equal(sliceMatches('cat', 'dog'), false)
  assert.equal(sliceMatches('  ', 'x'), false)
})
