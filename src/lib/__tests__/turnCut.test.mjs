// Unit tests for lib/turnCut.ts (the "Stopped mid-turn" mark, V3-29).
// Run: node --test src/lib/__tests__/turnCut.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { register } from 'node:module'

register(
  'data:text/javascript,' +
    encodeURIComponent(`
      export async function resolve(spec, ctx, next) {
        try { return await next(spec, ctx) } catch (e) {
          if (spec.startsWith('.') && !/\\.[cm]?[jt]s$/.test(spec)) return next(spec + '.ts', ctx)
          throw e
        }
      }`),
)
const { fallbackLastTurnCut, parseTurnCut, turnCutLabel, turnCutTime, turnCutTooltip } = await import('../turnCut.ts')
const { parseStreamLogWindow } = await import('../streamEvents.ts')

const t = (key, p) => `${key.replace('streamLog.turnCut.', '')}${p ? JSON.stringify(p) : ''}`
const now = new Date(2026, 10, 16, 12, 0, 0)
const today = new Date(2026, 10, 16, 10, 52, 3).toISOString()
const earlier = new Date(2026, 10, 14, 10, 52, 3).toISOString()

test('parseTurnCut keeps a known reason and a valid time, and blanks the rest', () => {
  assert.deepEqual(parseTurnCut({ reason: 'quota', at: today }), { reason: 'quota', at: today })
  assert.deepEqual(parseTurnCut({ reason: 'weird', at: 'not a date' }), { reason: '', at: '' })
  assert.deepEqual(parseTurnCut({}), { reason: '', at: '' })
})

test('turnCutTime shows the date only when it is not today', () => {
  const same = turnCutTime(today, now)
  const other = turnCutTime(earlier, now)
  assert.match(same, /\d/)
  assert.ok(other.length > same.length, `${other} vs ${same}`)
})

test('turnCutLabel words each reason, and falls back to the title alone', () => {
  for (const reason of ['user', 'quota', 'agent', 'app_closed']) {
    const label = turnCutLabel({ reason, at: today }, now, t)
    assert.ok(label.startsWith('title ' + reason + '{"time":"'), label)
  }
  assert.equal(turnCutLabel({ reason: '', at: '' }, now, t), 'title')
  assert.equal(turnCutLabel({ reason: 'user', at: '' }, now, t), 'title')
})

test('turnCutTooltip explains an app_closed time and is empty without one', () => {
  assert.ok(turnCutTooltip({ reason: 'app_closed', at: today }, t).startsWith('appClosedHint'))
  assert.ok(!turnCutTooltip({ reason: 'user', at: today }, t).startsWith('appClosedHint'))
  assert.equal(turnCutTooltip({ reason: 'user', at: '' }, t), '')
})

test('fallbackLastTurnCut marks only the last turn of a cancelled run without a mark', () => {
  const entries = [{ kind: 'user' }, { kind: 'text' }, { kind: 'user' }, { kind: 'text' }]
  const cancelled = { status: 'cancelled', end_reason: 'quota', finished_at: today }
  assert.deepEqual(fallbackLastTurnCut(entries, cancelled), { reason: 'quota', at: today })
  assert.deepEqual(fallbackLastTurnCut(entries, { status: 'cancelled', end_reason: null }), { reason: '', at: '' })
  assert.equal(fallbackLastTurnCut(entries, { status: 'done' }), null)
  assert.equal(fallbackLastTurnCut([], cancelled), null)
  assert.equal(fallbackLastTurnCut(entries, null), null)
})

test('fallbackLastTurnCut stays out when the last turn already has its mark', () => {
  const marked = [{ kind: 'user' }, { kind: 'text' }, { kind: 'turn_cut' }, { kind: 'result' }]
  assert.equal(fallbackLastTurnCut(marked, { status: 'cancelled' }), null)
  // A mark in an earlier turn does not cover the last one.
  const older = [{ kind: 'user' }, { kind: 'turn_cut' }, { kind: 'user' }, { kind: 'text' }]
  assert.notEqual(fallbackLastTurnCut(older, { status: 'cancelled' }), null)
})

test('the stream reader turns the record into a turn_cut entry, not a message', () => {
  const log = [
    JSON.stringify({ type: 'user', message: { role: 'user', content: 'go' } }),
    JSON.stringify({ type: '_puppeteer_turn_cut', reason: 'user', at: today }),
    JSON.stringify({ type: 'user', message: { role: 'user', content: 'again' } }),
  ]
  const { entries } = parseStreamLogWindow(log)
  assert.deepEqual(entries.map((e) => e.kind), ['user', 'turn_cut', 'user'])
  assert.deepEqual(entries[1], { kind: 'turn_cut', reason: 'user', at: today })
})
