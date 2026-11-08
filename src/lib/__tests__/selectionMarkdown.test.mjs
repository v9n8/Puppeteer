// Unit tests for lib/selectionMarkdown.ts (rendered selection → raw markdown).
// Run: node --test src/lib/__tests__/selectionMarkdown.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { rawMarkdownForSelection } from '../selectionMarkdown.ts'

const md = (src, selected) => rawMarkdownForSelection([src], selected)

test('a selection across a code fence keeps the fence and its language tag', () => {
  const src = 'Intro\n\n```ts\nconst x = 1\nconsole.log(x)\n```\n\nAfter'
  // The rendered view shows neither the fence nor `ts`, so the match needs a gap.
  assert.equal(md(src, 'Intro\nconst x = 1\nconsole.log(x)\nAfter'), src)
})

test('a selection inside a code fence returns the code lines verbatim', () => {
  const src = 'Intro\n\n```ts\nconst x = 1\nconsole.log(x)\n```'
  assert.equal(md(src, 'const x = 1\nconsole.log(x)'), 'const x = 1\nconsole.log(x)')
})

test('a whole table selected from its own block comes back as the table source', () => {
  const src = '| Name | Age |\n| --- | --- |\n| Ann | 30 |\n| Bob | 41 |'
  assert.equal(md(src, 'Name\tAge\nAnn\t30\nBob\t41'), src)
})

test('a table selected mid-document keeps the inner pipes and separator row', () => {
  const src = 'Text before\n\n| Name | Age |\n| --- | --- |\n| Ann | 30 |\n| Bob | 41 |\n\nAfter'
  // The slice ends at the last selected letter, so the closing ` |` of the last row is
  // not re-attached (`|` is not an emphasis marker).
  assert.equal(
    md(src, 'Text before\nName\tAge\nAnn\t30\nBob\t41'),
    'Text before\n\n| Name | Age |\n| --- | --- |\n| Ann | 30 |\n| Bob | 41',
  )
})

test('a selection crossing a link keeps the link syntax and URL', () => {
  const src = 'See [the docs](https://example.com/docs) for details.'
  assert.equal(md(src, 'See the docs for details.'), src)
})

test('a link URL longer than the gap limit is not bridged', () => {
  const src = `See [docs](https://example.com/${'z'.repeat(200)}) for details`
  assert.equal(md(src, 'See docs for details'), null)
})

test('nested list items keep their indent and bullet', () => {
  const src = '- Fruits\n  - Apple\n  - Pear\n- Veg'
  assert.equal(md(src, 'Apple\nPear'), '  - Apple\n  - Pear')
})

test('a selection at the start of a line pulls in its block marker', () => {
  assert.equal(md('Intro\n\n- first item\n- second', 'first item'), '- first item')
  assert.equal(md('Done:\n- [x] ship it', 'ship it'), '- [x] ship it')
  assert.equal(md('## Title\n\nBody text', 'Title\nBody text'), '## Title\n\nBody text')
})

test('a selection starting mid-line does not pull in the line marker', () => {
  assert.equal(md('- first item here', 'item here'), 'item here')
})

test('inline emphasis and code are kept when both sides carry the marker', () => {
  assert.equal(md('This is **very** important.', 'very'), '**very**')
  assert.equal(md('Run `pnpm install` first', 'pnpm install'), '`pnpm install`')
})

test('a one-sided marker (snake_case) is not absorbed', () => {
  assert.equal(md('call snake_case here', 'case'), 'case')
})

test('punctuation the selection ends on is re-attached', () => {
  assert.equal(md('Hello world. Next', 'Hello world.'), 'Hello world.')
  assert.equal(md('Hello world. Next', 'Hello world'), 'Hello world')
})

test('select-all of one source returns it with blank edges trimmed', () => {
  assert.equal(md('\n\n# Doc\n\nText\n\n', 'Doc\nText'), '# Doc\n\nText')
})

test('sources are tried in order; empty ones are skipped', () => {
  const sources = ['', 'unrelated words only', 'the **answer** is here']
  assert.equal(rawMarkdownForSelection(sources, 'answer'), '**answer**')
})

test('no match returns null so the caller falls back to the plain text', () => {
  const selected = 'nothing like this'
  assert.equal(md('Some other markdown', selected), null)
  assert.equal(md('Some other markdown', selected) ?? selected, selected)
})

test('a selection with fewer than two word characters returns null', () => {
  assert.equal(md('a b c', 'a'), null)
  assert.equal(md('a b c', '—'), null)
})
