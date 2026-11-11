// Unit tests for lib/fileLinks.ts: which text references in a session resolve
// to a project file, and the line they point at (UX-43); bare paths in prose and
// the batched existence check behind them (UX-44).
// Run: node --test src/lib/__tests__/fileLinks.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import MarkdownIt from 'markdown-it'
import {
  createPathChecker, findPlainPaths, hrefFileRef, matchProjectFile, parseLineRef, splitLineRef,
} from '../fileLinks.ts'

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

const found = (text) => findPlainPaths(text).map((p) => [p.text, p.path, p.line])

test('findPlainPaths: the forms a sentence names a file in', () => {
  assert.deepEqual(found('Xem README.md nhé.'), [['README.md', 'README.md', null]])
  assert.deepEqual(found('see src/a.ts:12, then a.ts:12-20.'), [['src/a.ts:12', 'src/a.ts', 12], ['a.ts:12-20', 'a.ts', 12]])
  assert.deepEqual(found('at src/a.ts:12:5 and src/b.ts#L3-L9'), [['src/a.ts:12:5', 'src/a.ts', 12], ['src/b.ts#L3-L9', 'src/b.ts', 3]])
  assert.deepEqual(found('wrote /tmp/out/plan.md'), [['/tmp/out/plan.md', '/tmp/out/plan.md', null]])
  assert.deepEqual(found('[a](src/a.ts#L12)'), [['src/a.ts#L12', 'src/a.ts', 12]])
  assert.deepEqual(found('(./src/a.ts) and ../lib/x.vue'), [['./src/a.ts', './src/a.ts', null], ['../lib/x.vue', '../lib/x.vue', null]])
  assert.deepEqual(found('.github/workflows/ci.yml'), [['.github/workflows/ci.yml', '.github/workflows/ci.yml', null]])
  assert.deepEqual(found('ファイルsrc/a.tsを開く'), [['src/a.ts', 'src/a.ts', null]])
  assert.deepEqual(found('Path: src/a.ts: done'), [['src/a.ts', 'src/a.ts', null]])
  const [p] = findPlainPaths('in x/a.ts.')
  assert.equal('in x/a.ts.'.slice(p.start, p.end), 'x/a.ts')
})

test('findPlainPaths: what must not read as a path', () => {
  for (const text of [
    'array.map(fn)', 'version 1.2.3', 'e.g. this', 'https://example.com/a/b.ts', 'http://x.io/README.md',
    'www.example.com', 'foo.tsz', 'foo.ts-bar', '~/notes/a.md', 'C:/x/a.ts', 'just words',
  ]) {
    assert.deepEqual(found(text), [], text)
  }
})

test('createPathChecker: one batched call per tick, cached, split at the cap', async () => {
  const calls = []
  const checker = createPathChecker(async (paths) => {
    calls.push(paths)
    return paths.map((p) => p.startsWith('yes'))
  }, 2)
  assert.equal(checker.known('yes1'), undefined)
  await Promise.all([checker.check(['yes1', 'no1']), checker.check(['yes2', 'yes1'])])
  assert.deepEqual(calls, [['yes1', 'no1'], ['yes2']])
  assert.equal(checker.known('yes1'), true)
  assert.equal(checker.known('no1'), false)
  await checker.check(['yes1', 'no1', 'yes2'])
  assert.equal(calls.length, 2) // answered from the cache
})

test('createPathChecker: clear forgets answers and drops the ones in flight', async () => {
  let release
  const gate = new Promise((r) => { release = r })
  let n = 0
  const checker = createPathChecker(async (paths) => {
    n++
    if (n === 1) await gate
    return paths.map(() => true)
  })
  const first = checker.check(['a.ts'])
  await Promise.resolve()
  checker.clear()
  release()
  await first
  assert.equal(checker.known('a.ts'), undefined) // asked before the clear: dropped
  await checker.check(['a.ts'])
  assert.equal(checker.known('a.ts'), true)
  assert.equal(n, 2)
})

test('createPathChecker: a failed call caches nothing and rejects', async () => {
  let fail = true
  const checker = createPathChecker(async (paths) => {
    if (fail) throw new Error('down')
    return paths.map(() => true)
  })
  await assert.rejects(checker.check(['a.ts']))
  assert.equal(checker.known('a.ts'), undefined)
  fail = false
  await checker.check(['a.ts'])
  assert.equal(checker.known('a.ts'), true)
})
