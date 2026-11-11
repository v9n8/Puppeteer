// Unit tests for lib/quotaSwitch.ts (accounts offered when a quota blocks a session).
// Run: node --test src/lib/__tests__/quotaSwitch.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { quotaSwitchTargets } from '../quotaSwitch.ts'

const NOW = Date.parse('2026-11-10T12:00:00Z')
const LATER = '2026-11-10T14:30:00Z'
const PAST = '2026-11-10T11:00:00Z'
const snap = (five, week, extra = {}) => ({
  captured_at: '2026-11-10T11:58:00Z',
  rate_limits_available: true,
  windows: {
    five_hour: five == null ? null : { utilization: five, resets_at: extra.fiveReset ?? LATER, status: extra.status ?? null },
    seven_day: week == null ? null : { utilization: week, resets_at: LATER, status: null },
  },
})
const th = (block5hPercent = 0, blockWeekPercent = 0) => ({ warnPercent: 0, block5hPercent, blockWeekPercent })
const acc = (id, engine = 'claude') => ({ value: id, label: id.toUpperCase(), usageKey: `${engine}:${id}` })
const ids = (list) => list.map((a) => a.value)

test('offers the accounts under their block thresholds, lowest usage first', () => {
  const accounts = [acc('a'), acc('b'), acc('c'), acc('d')]
  const snapshots = {
    'claude:a': snap(95, 40), // the run's own account
    'claude:b': snap(70, 20),
    'claude:c': snap(10, 50), // its weekly window is what counts
    'claude:d': snap(30, 10),
  }
  assert.deepEqual(ids(quotaSwitchTargets(accounts, 'claude:a', snapshots, th(90, 90), NOW)), ['d', 'c', 'b'])
})

test('skips an account over a block threshold, blocked by the plan, or at 100%', () => {
  const accounts = [acc('a'), acc('b'), acc('c'), acc('d'), acc('e')]
  const snapshots = {
    'claude:b': snap(85, 10), // 5h over 80
    'claude:c': snap(10, 92), // weekly over 90
    'claude:d': snap(20, 10, { status: 'blocked' }),
    'claude:e': snap(100, 10), // no 5h threshold set, but the plan is spent
  }
  assert.deepEqual(ids(quotaSwitchTargets(accounts, 'claude:a', snapshots, th(80, 90), NOW)), [])
  // With no 5h threshold, 85% is fine; a blocked or spent plan still is not.
  assert.deepEqual(ids(quotaSwitchTargets(accounts, 'claude:a', snapshots, th(0, 90), NOW)), ['b'])
})

test('a window that already reset counts as 0%', () => {
  const accounts = [acc('a'), acc('b'), acc('c')]
  const snapshots = {
    'claude:b': snap(99, 20, { fiveReset: PAST }),
    'claude:c': snap(30, 30),
  }
  assert.deepEqual(ids(quotaSwitchTargets(accounts, 'claude:a', snapshots, th(90, 90), NOW)), ['b', 'c'])
})

test('accounts with no plan figures come last, in their own order', () => {
  const accounts = [acc('a'), acc('b'), acc('c'), acc('d')]
  const snapshots = {
    'claude:b': null, // never ran
    'claude:c': { captured_at: null, rate_limits_available: false, windows: {} }, // API key
    'claude:d': snap(60, 60),
  }
  assert.deepEqual(ids(quotaSwitchTargets(accounts, 'claude:a', snapshots, th(90, 90), NOW)), ['d', 'b', 'c'])
})

test('never offers the run\'s own account; the shared Codex login is a choice like any other', () => {
  const shared = { value: '', label: 'Shared Codex login', usageKey: 'codex' }
  const accounts = [shared, acc('x', 'codex'), acc('y', 'codex')]
  const snapshots = { codex: snap(5, 5), 'codex:x': snap(95, 50), 'codex:y': snap(20, 20) }
  assert.deepEqual(ids(quotaSwitchTargets(accounts, 'codex:x', snapshots, th(90, 90), NOW)), ['', 'y'])
  assert.deepEqual(ids(quotaSwitchTargets(accounts, 'codex', snapshots, th(90, 90), NOW)), ['y'])
  assert.deepEqual(ids(quotaSwitchTargets([shared], 'codex', snapshots, th(90, 90), NOW)), [])
})
