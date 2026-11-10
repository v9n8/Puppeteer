// Unit tests for scripts/i18n-glossary.mjs (vi terms that stay in English).
// Run: node --test src/lib/__tests__/i18nGlossary.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { findBannedTerms } from '../../../scripts/i18n-glossary.mjs'

const GLOSSARY = {
  terms: [
    { term: 'Session', banned: ['phiên'], ignore: ['phiên bản'] },
    { term: 'Project', banned: ['dự án'] },
    { term: 'Fork', banned: ['phân nhánh'] },
    { term: 'Branch', banned: ['nhánh'] },
  ],
  allow: [{ key: 'settings.azure.intro', phrase: 'phiên az login', reason: 'az CLI login' }],
}

const find = (message, key = 'run.title') => findBannedTerms(message, GLOSSARY, key)

test('reports a banned word with the term to use, keeping its case', () => {
  assert.deepEqual(find('Tạo phiên mới'), [{ word: 'phiên', term: 'Session' }])
  assert.deepEqual(find('Phiên đã dừng'), [{ word: 'Phiên', term: 'Session' }])
  assert.deepEqual(find('DỰ ÁN'), [{ word: 'DỰ ÁN', term: 'Project' }])
})

test('reports every occurrence', () => {
  assert.deepEqual(find('phiên của dự án, phiên khác'), [
    { word: 'phiên', term: 'Session' },
    { word: 'phiên', term: 'Session' },
    { word: 'dự án', term: 'Project' },
  ])
})

test('the English term itself is fine', () => {
  assert.deepEqual(find('Tạo Session mới trong Project'), [])
})

test('only whole words count, also next to Vietnamese letters', () => {
  // "nhánh" inside a longer word, and a banned word glued to letters with diacritics.
  assert.deepEqual(find('nhánhđ'), [])
  assert.deepEqual(find('ănhánh'), [])
  assert.deepEqual(find('(phiên)'), [{ word: 'phiên', term: 'Session' }])
  assert.deepEqual(find('{count} phiên'), [{ word: 'phiên', term: 'Session' }])
})

test('decomposed (NFD) text matches like composed text', () => {
  assert.deepEqual(find('dự án'.normalize('NFD')), [{ word: 'dự án', term: 'Project' }])
})

test('an ignore phrase hides the banned word inside it', () => {
  assert.deepEqual(find('Phiên bản 1.2'), [])
  assert.deepEqual(find('phiên bản của phiên'), [{ word: 'phiên', term: 'Session' }])
})

test('the longer banned phrase wins, so it is reported once', () => {
  assert.deepEqual(find('phân nhánh từ nhánh chính'), [
    { word: 'phân nhánh', term: 'Fork' },
    { word: 'nhánh', term: 'Branch' },
  ])
})

test('an allow entry exempts its phrase only in its own key', () => {
  const message = 'Mỗi tài khoản là một phiên az login riêng, dùng trong phiên'
  assert.deepEqual(find(message, 'settings.azure.intro'), [{ word: 'phiên', term: 'Session' }])
  assert.deepEqual(find(message, 'settings.aws.intro'), [
    { word: 'phiên', term: 'Session' },
    { word: 'phiên', term: 'Session' },
  ])
})

test('the shipped glossary lists every N4 term and only well-formed entries', () => {
  const glossary = JSON.parse(
    readFileSync(new URL('../../../scripts/i18n-glossary.json', import.meta.url), 'utf8'),
  )
  const terms = glossary.terms.map((t) => t.term)
  for (const term of [
    'Session', 'Skill', 'Rule', 'Command', 'Agent', 'Hook', 'MCP', 'Prompt', 'Token',
    'Context', 'Model', 'Engine', 'Effort', 'Quote', 'Fork', 'Rewind', 'Steer', 'Queue',
    'Worktree', 'Branch', 'Commit', 'PR', 'Issue', 'Conductor', 'Project',
  ]) {
    assert.ok(terms.includes(term), `missing term ${term}`)
  }
  for (const t of glossary.terms) assert.ok(Array.isArray(t.banned), `${t.term}.banned`)
  for (const a of glossary.allow) {
    assert.ok(a.key && a.phrase && a.reason, `allow entry needs key, phrase, reason: ${JSON.stringify(a)}`)
  }
  assert.deepEqual(findBannedTerms('phiên bản mới của dự án', glossary, 'x.y'), [
    { word: 'dự án', term: 'Project' },
  ])
})
