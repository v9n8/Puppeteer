// Unit tests for lib/permissionOrigin.ts (the "who asked" line of a permission prompt).
// Run: node --test src/lib/__tests__/permissionOrigin.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { engineLabel, engineName, permissionOriginParts } from '../permissionOrigin.ts'

const labels = {
  justNow: 'just now',
  main: 'main agent',
  sub: 'sub-agent',
  subNamed: (name) => `sub-agent “${name}”`,
}
// Local wall-clock times, so the HH:mm check holds in any time zone.
const asked = new Date(2026, 10, 18, 10, 51, 0)
const at = asked.toISOString()
const plus = (ms) => new Date(asked.getTime() + ms)
const texts = (parts) => parts.map((p) => p.text)
const line = (req, now = plus(30_000), locale = 'en') => texts(permissionOriginParts(req, now, locale, labels))

test('all four fields: engine · short model · agent · HH:mm · relative', () => {
  const parts = permissionOriginParts(
    { engine: 'claude', model: 'claude-opus-4-5', agent: { kind: 'main' }, requested_at: at },
    plus(30_000),
    'en',
    labels,
  )
  assert.deepEqual(texts(parts), ['Claude', 'opus-4-5', 'main agent', '10:51', 'just now'])
  assert.equal(parts.find((p) => p.key === 'model').title, 'claude-opus-4-5')
})

test('relative label: just now under a minute, then minutes via Intl', () => {
  const req = { requested_at: at }
  assert.deepEqual(line(req, plus(59_000)), ['10:51', 'just now'])
  assert.deepEqual(line(req, plus(60_000)), ['10:51', '1 minute ago'])
  assert.deepEqual(line(req, plus(5 * 60_000)), ['10:51', '5 minutes ago'])
  assert.deepEqual(line(req, plus(3 * 60_000), 'vi'), ['10:51', '3 phút trước'])
  // A clock a little behind the request (minute ticks) still reads "just now".
  assert.deepEqual(line(req, plus(-20_000)), ['10:51', 'just now'])
})

test('each missing field drops only its own part', () => {
  const full = { engine: 'claude', model: 'claude-opus-4-5', agent: { kind: 'main' }, requested_at: at }
  assert.deepEqual(line({ ...full, engine: undefined }), ['opus-4-5', 'main agent', '10:51', 'just now'])
  assert.deepEqual(line({ ...full, model: null }), ['Claude', 'main agent', '10:51', 'just now'])
  assert.deepEqual(line({ ...full, agent: undefined }), ['Claude', 'opus-4-5', '10:51', 'just now'])
  assert.deepEqual(line({ ...full, requested_at: undefined }), ['Claude', 'opus-4-5', 'main agent'])
})

test('none of the four: no parts, so no line is drawn', () => {
  assert.deepEqual(line({}), [])
  assert.deepEqual(line({ engine: null, model: '', agent: null, requested_at: null }), [])
})

test('codex: no agent part and never the word Claude; only the claude- prefix is dropped', () => {
  const parts = line({ engine: 'codex', model: 'gpt-5.1-codex', requested_at: at })
  assert.deepEqual(parts, ['Codex', 'gpt-5.1-codex', '10:51', 'just now'])
  assert.ok(!parts.join(' ').includes('Claude'))
})

test('sub-agent: named only when the source has a name', () => {
  const parts = permissionOriginParts({ agent: { kind: 'sub', id: 'ag-1' } }, plus(0), 'en', labels)
  assert.deepEqual(texts(parts), ['sub-agent'])
  assert.equal(parts[0].title, 'ag-1')
  assert.deepEqual(line({ agent: { kind: 'sub', name: 'researcher' } }), ['sub-agent “researcher”'])
})

test('unknown engine and a broken timestamp are dropped', () => {
  assert.deepEqual(line({ engine: 'gemini', requested_at: 'not a date' }), [])
})

test('engineName / engineLabel: fallback without inventing Claude', () => {
  assert.equal(engineName('claude'), 'Claude')
  assert.equal(engineName('codex'), 'Codex')
  assert.equal(engineName(undefined), null)
  assert.equal(engineName(''), null)
  const pick = (e) => engineLabel(e, (name) => `${name} requests permission`, 'Permission requested')
  assert.equal(pick('claude'), 'Claude requests permission')
  assert.equal(pick('codex'), 'Codex requests permission')
  assert.equal(pick(null), 'Permission requested')
  assert.equal(pick('other'), 'Permission requested')
})
