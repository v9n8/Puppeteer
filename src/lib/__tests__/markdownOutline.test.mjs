// Unit tests for lib/markdownOutline.ts (viewer outline + compare kind).
// Run: node --test src/lib/__tests__/markdownOutline.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { compareKind, markdownOutline } from '../markdownOutline.ts'

test('lists ATX headings with level, text and slug', () => {
  const out = markdownOutline('# Title\n\ntext\n\n## Second part\n### Third\n')
  assert.deepEqual(out, [
    { level: 1, text: 'Title', slug: 'title' },
    { level: 2, text: 'Second part', slug: 'second-part' },
    { level: 3, text: 'Third', slug: 'third' },
  ])
})

test('ignores headings inside fenced code blocks', () => {
  const src = '# A\n```sh\n# not a heading\n```\n~~~\n## nor this\n~~~\n## B\n'
  assert.deepEqual(markdownOutline(src).map(i => i.text), ['A', 'B'])
})

test('a longer fence is not closed by a shorter one', () => {
  const src = '````\n```\n# inside\n````\n# out\n'
  assert.deepEqual(markdownOutline(src).map(i => i.text), ['out'])
})

test('duplicate headings get unique slugs', () => {
  const out = markdownOutline('# Notes\n## Notes\n## Notes\n')
  assert.deepEqual(out.map(i => i.slug), ['notes', 'notes-1', 'notes-2'])
})

test('strips inline markdown and closing hashes; keeps non-latin letters', () => {
  const out = markdownOutline('## `code` and [link](http://x.y) ##\n# Đăng nhập SSO\n')
  assert.equal(out[0].text, 'code and link')
  assert.equal(out[1].slug, 'đăng-nhập-sso')
})

test('requires a space after the hashes and skips empty headings', () => {
  assert.deepEqual(markdownOutline('#nospace\n#\n####### seven\n'), [])
})

test('compareKind: image, text and not comparable', () => {
  assert.equal(compareKind('a/b/logo.PNG'), 'image')
  assert.equal(compareKind('src/session.ts'), 'text')
  assert.equal(compareKind('README.md'), 'text')
  assert.equal(compareKind('Makefile'), 'text')
  assert.equal(compareKind('doc/spec.pdf'), null)
  assert.equal(compareKind('clip.mp4'), null)
  assert.equal(compareKind('build/app.exe'), null)
})
