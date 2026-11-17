// Unit tests for lib/mentionSegments.ts (the `@path` chips behind the composer textarea).
// Run: node --test src/lib/__tests__/mentionSegments.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { splitMentions, MENTION_OVERLAY_MAX_CHARS } from '../mentionSegments.ts'
import { matchProjectFile } from '../fileLinks.ts'

const FILES = new Set(['src/a.ts', 'src/b.ts', 'docs/ghi-chú.md'])
const resolve = (raw) => matchProjectFile(raw, FILES)
const split = (text) => splitMentions(text, resolve)
const chips = (text) => split(text).filter((s) => s.mention).map((s) => s.text)
const joined = (segs) => segs.map((s) => s.text).join('')

test('empty text has no segments', () => {
  assert.deepEqual(split(''), [])
})

test('text without a mention stays one plain segment', () => {
  assert.deepEqual(split('fix the login bug'), [{ text: 'fix the login bug', mention: false }])
})

test('a mention at the very start is a chip', () => {
  assert.deepEqual(split('@src/a.ts please'), [
    { text: '@src/a.ts', mention: true },
    { text: ' please', mention: false },
  ])
})

test('a mention after a space or a newline is a chip', () => {
  assert.deepEqual(chips('look at @src/a.ts'), ['@src/a.ts'])
  assert.deepEqual(chips('first line\n@src/a.ts second'), ['@src/a.ts'])
  assert.deepEqual(chips('tab\t@src/a.ts'), ['@src/a.ts'])
})

test('an @ inside a word is not a mention (email)', () => {
  assert.deepEqual(chips('mail a@src/a.ts or a@b.com'), [])
})

test('a token the resolver rejects stays plain', () => {
  assert.deepEqual(split('see @xyz now'), [{ text: 'see @xyz now', mention: false }])
  assert.deepEqual(chips('folder @src/ here'), [])
  assert.deepEqual(chips('just @ alone'), [])
})

test('closing punctuation stays outside the chip', () => {
  assert.deepEqual(split('@src/a.ts, then'), [
    { text: '@src/a.ts', mention: true },
    { text: ', then', mention: false },
  ])
  assert.deepEqual(chips('(see @src/a.ts).'), ['@src/a.ts'])
  assert.deepEqual(chips('(@src/a.ts)'), [])
})

test('a line reference stays inside the chip', () => {
  assert.deepEqual(chips('at @src/a.ts:12 there'), ['@src/a.ts:12'])
})

test('two mentions next to each other are two chips', () => {
  assert.deepEqual(split('@src/a.ts @src/b.ts'), [
    { text: '@src/a.ts', mention: true },
    { text: ' ', mention: false },
    { text: '@src/b.ts', mention: true },
  ])
  assert.deepEqual(chips('@src/a.ts\n@src/b.ts'), ['@src/a.ts', '@src/b.ts'])
})

test('Vietnamese text with combining marks keeps every character', () => {
  const text = 'Sửa lỗi ở @docs/ghi-chú.md giúp mình'
  assert.deepEqual(chips(text), ['@docs/ghi-chú.md'])
  const decomposed = 'Sửa lỗi'.normalize('NFD') + ' @src/a.ts ' + 'nhé'.normalize('NFD')
  assert.equal(joined(split(decomposed)), decomposed)
  assert.deepEqual(chips(decomposed), ['@src/a.ts'])
})

test('text longer than the cap is one plain segment', () => {
  const text = '@src/a.ts ' + 'x'.repeat(MENTION_OVERLAY_MAX_CHARS)
  assert.equal(MENTION_OVERLAY_MAX_CHARS, 20000)
  assert.deepEqual(split(text), [{ text, mention: false }])
  const atCap = '@src/a.ts ' + 'x'.repeat(MENTION_OVERLAY_MAX_CHARS - 10)
  assert.deepEqual(chips(atCap), ['@src/a.ts'])
})

test('the segments always join back to the input', () => {
  for (const text of ['@src/a.ts', ' @src/a.ts ', 'a@b.com @src/a.ts, @xyz\n@src/b.ts:3).', '@@src/a.ts', '@src/a.ts@src/b.ts']) {
    assert.equal(joined(split(text)), text)
  }
})
