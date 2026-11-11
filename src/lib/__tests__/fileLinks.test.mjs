// Unit tests for lib/fileLinks.ts: which text references in a session resolve
// to a project file, and the line they point at (UX-43).
// Run: node --test src/lib/__tests__/fileLinks.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import MarkdownIt from 'markdown-it'
import { hrefFileRef, matchProjectFile, parseLineRef, splitLineRef } from '../fileLinks.ts'

const FILES = new Set(['src/a.ts', 'src/lib/b.ts', 'docs/README.md', 'README.md', 'x/dup.ts', 'y/dup.ts'])
const ROOT = '/Users/me/proj'
const match = (raw) => matchProjectFile(raw, FILES, ROOT)

test('matchProjectFile: plain, ./ and suffix paths', () => {
  assert.equal(match('src/a.ts'), 'src/a.ts')
  assert.equal(match('./src/a.ts'), 'src/a.ts')
  assert.equal(match('lib/b.ts'), 'src/lib/b.ts')
  assert.equal(match('b.ts'), 'src/lib/b.ts')
})

test('matchProjectFile: a line reference does not stop the match', () => {
  for (const raw of ['src/a.ts:12', 'src/a.ts:12:5', 'a.ts:12-20', 'src/a.ts#L12', 'src/a.ts#L12-L20', 'src/a.ts:12,']) {
    assert.equal(match(raw), 'src/a.ts', raw)
  }
})

test('matchProjectFile: an absolute path inside the project matches as relative', () => {
  assert.equal(match(`${ROOT}/src/a.ts`), 'src/a.ts')
  assert.equal(match(`${ROOT}/src/a.ts:12`), 'src/a.ts')
  assert.equal(matchProjectFile(`${ROOT}/src/a.ts`, FILES, `${ROOT}/`), 'src/a.ts')
})

test('matchProjectFile: absolute paths outside the project, ambiguous names and non-files do not match', () => {
  assert.equal(match('/Users/me/other/src/a.ts'), null)
  assert.equal(match(`${ROOT}-copy/src/a.ts`), null)
  assert.equal(matchProjectFile(`${ROOT}/src/a.ts`, FILES), null) // no root given
  assert.equal(match('dup.ts'), null)
  assert.equal(match('x/dup.ts'), 'x/dup.ts')
  assert.equal(match('missing.ts'), null)
  assert.equal(match('array.map'), null)
  assert.equal(match('src/a.ts and more'), null)
  assert.equal(match(''), null)
})

test('splitLineRef / parseLineRef: line, line:column, ranges and anchors', () => {
  assert.deepEqual(splitLineRef('a.ts:12'), { path: 'a.ts', line: 12 })
  assert.deepEqual(splitLineRef('a.ts:12:5'), { path: 'a.ts', line: 12 })
  assert.deepEqual(splitLineRef('a.ts:12-20'), { path: 'a.ts', line: 12 })
  assert.deepEqual(splitLineRef('a.ts#L12-L20'), { path: 'a.ts', line: 12 })
  assert.deepEqual(splitLineRef('a.ts#42'), { path: 'a.ts', line: 42 })
  assert.deepEqual(splitLineRef('/abs/a.ts'), { path: '/abs/a.ts', line: null })
  assert.equal(parseLineRef('src/a.ts:7:3`'), 7)
  assert.equal(parseLineRef('README.md#install'), null)
  assert.equal(parseLineRef('src/a.ts'), null)
})

test('hrefFileRef: markdown link targets', () => {
  assert.deepEqual(hrefFileRef('src/a.ts#L12'), { path: 'src/a.ts', line: 12 })
  assert.deepEqual(hrefFileRef('/abs/a.ts:12'), { path: '/abs/a.ts', line: 12 })
  assert.deepEqual(hrefFileRef('README.md#install'), { path: 'README.md', line: null })
  assert.deepEqual(hrefFileRef('/Users/v%C5%A9/a.ts'), { path: '/Users/vũ/a.ts', line: null })
  assert.deepEqual(hrefFileRef('a%E0.ts'), { path: 'a%E0.ts', line: null }) // malformed escape kept
  assert.deepEqual(hrefFileRef('C:/a.ts'), { path: 'C:/a.ts', line: null })
  assert.equal(hrefFileRef('#section'), null)
  assert.equal(hrefFileRef('https://example.com/a.ts'), null)
  assert.equal(hrefFileRef('vscode://file/a.ts'), null)
})

// The session renderer turns off linkify's fuzzy matching: `.md` / `.rs` are
// country-code domains, so `README.md` became http://README.md and opened the browser.
test('session markdown: README.md stays text, URLs with a scheme still link', () => {
  const src = readFileSync(new URL('../markdown.ts', import.meta.url), 'utf8')
  assert.match(src, /_md\.linkify\.set\(\{ fuzzyLink: false \}\)/)
  const md = new MarkdownIt({ html: false, linkify: true, typographer: true, breaks: true })
  md.linkify.set({ fuzzyLink: false })
  const html = md.render('See README.md and main.rs, then https://example.com/x')
  assert.doesNotMatch(html, /http:\/\/README\.md|http:\/\/main\.rs/)
  assert.match(html, /<a href="https:\/\/example\.com\/x">/)
  assert.match(md.render('[a](/abs/a.ts:12)'), /<a href="\/abs\/a\.ts:12">/)
})
