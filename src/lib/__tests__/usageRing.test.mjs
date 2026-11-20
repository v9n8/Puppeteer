// Unit tests for lib/usageRing.ts (usage rings on the session status bar).
// Run: node --test src/lib/__tests__/usageRing.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  usageWindows, ringWindow, ringSeverity, windowSeverity, parsePercentSetting, resetCountdown, usageSummary,
  visibleUsageRings, topWindow, usageColumns, barCell, barRows, currentUsageKey,
  planUsageRead,
} from '../usageRing.ts'

const NOW = Date.parse('2026-11-10T12:00:00Z')
const LATER = '2026-11-10T14:30:00Z'
const PAST = '2026-11-10T11:00:00Z'
const win = (utilization, resets_at = LATER, status = null) => ({ utilization, resets_at, status })
const snap = (windows, rate_limits_available = true) => ({ captured_at: '2026-11-10T11:58:00Z', rate_limits_available, windows })
const th = (warnPercent = 0, block5hPercent = 0, blockWeekPercent = 0) => ({ warnPercent, block5hPercent, blockWeekPercent })
const w = (key, percent, extra = {}) => ({
  key, period: key === 'five_hour' ? '5h' : 'week', percent, reset: LATER, rolledOver: false, blocked: false, ...extra,
})

test('usageWindows keeps windows with data, in display order', () => {
  const list = usageWindows(snap({
    seven_day_opus: win(12.4),
    seven_day: win(40.6),
    five_hour: win(7),
    seven_day_sonnet: win(null),
  }), NOW)
  assert.deepEqual(list.map((x) => [x.key, x.period, x.percent]), [
    ['five_hour', '5h', 7], ['seven_day', 'week', 41], ['seven_day_opus', 'week', 12],
  ])
})

test('usageWindows: no data → no windows (the ring hides)', () => {
  assert.deepEqual(usageWindows(null, NOW), [])
  assert.deepEqual(usageWindows(snap({ five_hour: win(50) }, false), NOW), [])
  assert.deepEqual(usageWindows(snap({ five_hour: win(null), seven_day: null }), NOW), [])
})

test('usageWindows: a window past its reset counts as 0% and never blocks', () => {
  const [five] = usageWindows(snap({ five_hour: win(97, PAST, 'blocked') }), NOW)
  assert.equal(five.percent, 0)
  assert.equal(five.rolledOver, true)
  assert.equal(five.blocked, false)
  assert.equal(windowSeverity(five, th(50, 50)), 'ok')
})

test('ringWindow: the 5h window, else the most used one', () => {
  assert.equal(ringWindow([w('seven_day', 90), w('five_hour', 10)]).key, 'five_hour')
  assert.equal(ringWindow([w('seven_day', 30), w('seven_day_opus', 60)]).key, 'seven_day_opus')
  assert.equal(ringWindow([]), null)
})

test('windowSeverity: amber at 80% while the quota warning tier is off', () => {
  assert.equal(windowSeverity(w('five_hour', 79), th()), 'ok')
  assert.equal(windowSeverity(w('five_hour', 80), th()), 'warn')
  assert.equal(windowSeverity(w('five_hour', 100), th()), 'over')
})

test('windowSeverity: amber follows quota_warn_percent when set', () => {
  assert.equal(windowSeverity(w('five_hour', 60), th(60)), 'warn')
  assert.equal(windowSeverity(w('five_hour', 59), th(60)), 'ok')
  // A tier above 80 moves amber up too.
  assert.equal(windowSeverity(w('seven_day', 85), th(90)), 'ok')
})

test('windowSeverity: red at the block threshold of the window period', () => {
  assert.equal(windowSeverity(w('five_hour', 70), th(60, 70, 0)), 'over')
  assert.equal(windowSeverity(w('seven_day', 70), th(60, 70, 0)), 'warn', '5h threshold does not apply to weekly')
  assert.equal(windowSeverity(w('seven_day_opus', 95), th(0, 0, 95)), 'over')
  assert.equal(windowSeverity(w('five_hour', 20, { blocked: true }), th()), 'over', 'plan reported blocked')
})

test('usageSummary: the 5h window, then the most used weekly one', () => {
  assert.deepEqual(
    usageSummary([w('five_hour', 30), w('seven_day', 20), w('seven_day_opus', 60), w('seven_day_sonnet', 10)]).map((x) => x.key),
    ['five_hour', 'seven_day_opus'],
  )
  assert.deepEqual(usageSummary([w('seven_day', 40)]).map((x) => x.key), ['seven_day'], 'Codex: weekly only')
  assert.deepEqual(usageSummary([w('five_hour', 5)]).map((x) => x.key), ['five_hour'])
  assert.deepEqual(usageSummary([w('seven_day', 50), w('seven_day_opus', 50)]).map((x) => x.key), ['seven_day'], 'a tie keeps the first')
  assert.deepEqual(usageSummary([]), [])
})

test('ringSeverity: the worst window colours the ring', () => {
  assert.equal(ringSeverity([w('five_hour', 10), w('seven_day', 96)], th(0, 0, 95)), 'over')
  assert.equal(ringSeverity([w('five_hour', 10), w('seven_day', 85)], th()), 'warn')
  assert.equal(ringSeverity([w('five_hour', 10)], th()), 'ok')
  assert.equal(ringSeverity([], th()), 'ok')
})

test('parsePercentSetting matches the backend: empty / junk = off, clamped to 0–100', () => {
  assert.equal(parsePercentSetting(''), 0)
  assert.equal(parsePercentSetting(undefined), 0)
  assert.equal(parsePercentSetting('abc'), 0)
  assert.equal(parsePercentSetting(' 85 '), 85)
  assert.equal(parsePercentSetting('150'), 100)
  assert.equal(parsePercentSetting('-5'), 0)
  assert.equal(parsePercentSetting('85.5'), 0)
  assert.equal(parsePercentSetting('85%'), 0)
})

test('resetCountdown: unknown, due, minutes, hours, days', () => {
  assert.equal(resetCountdown(null, NOW), null)
  assert.equal(resetCountdown('not a date', NOW), null)
  assert.deepEqual(resetCountdown(PAST, NOW), { due: true })
  assert.deepEqual(resetCountdown('2026-11-10T12:00:20Z', NOW), { due: false, days: 0, hours: 0, minutes: 1 }, 'rounds up')
  assert.deepEqual(resetCountdown('2026-11-10T12:45:00Z', NOW), { due: false, days: 0, hours: 0, minutes: 45 })
  assert.deepEqual(resetCountdown(LATER, NOW), { due: false, days: 0, hours: 2, minutes: 30 })
  assert.deepEqual(resetCountdown('2026-11-13T16:10:00Z', NOW), { due: false, days: 3, hours: 4, minutes: 0 })
})

test('planUsageRead: each row key reads its own login snapshot', () => {
  assert.deepEqual(planUsageRead('claude'), { cmd: 'get_plan_usage' })
  assert.deepEqual(planUsageRead('claude:a1'), { cmd: 'get_plan_usage', args: { accountId: 'a1' } })
  assert.deepEqual(planUsageRead('codex'), { cmd: 'get_plan_usage_codex' })
  assert.deepEqual(planUsageRead('codex:x1'), { cmd: 'get_plan_usage_codex', args: { accountId: 'x1' } })
})

test('visibleUsageRings: every account shows, data or not; global rows wait for data', () => {
  const w = [{ key: 'five_hour', period: '5h', percent: 3, reset: null, rolledOver: false, blocked: false }]
  const rows = [
    { key: 'claude:a', windows: w },
    { key: 'claude:b', windows: [] },
    { key: 'codex', windows: [] },
    { key: 'codex:c', windows: [] },
    { key: 'claude', windows: w },
  ]
  assert.deepEqual(visibleUsageRings(rows).map((r) => r.key), ['claude:a', 'claude:b', 'codex:c', 'claude'])
})

// ── DS-1: one usage block ──
const rowOf = (key, windows) => ({ key, windows })
const keysOf = (list) => list.map((r) => r.key)

test('topWindow: the most used window, a tie keeps the 5h; none without data', () => {
  assert.equal(topWindow([w('five_hour', 12), w('seven_day', 42)]).key, 'seven_day', 'week above 5h')
  assert.equal(topWindow([w('five_hour', 86), w('seven_day', 61)]).key, 'five_hour')
  assert.equal(topWindow([w('five_hour', 40), w('seven_day', 40)]).key, 'five_hour', 'tie keeps 5h')
  assert.equal(topWindow([w('five_hour', 10), w('seven_day', 30), w('seven_day_opus', 97)]).key, 'seven_day_opus')
  assert.equal(topWindow([]), null, 'no data')
})

test('usageColumns: 5h, the most used weekly window, and every weekly one for the tooltip', () => {
  const c = usageColumns([w('five_hour', 5), w('seven_day', 20), w('seven_day_opus', 60), w('seven_day_sonnet', 10)])
  assert.equal(c.fiveHour.percent, 5)
  assert.equal(c.week.key, 'seven_day_opus')
  assert.deepEqual(keysOf(c.weekAll), ['seven_day', 'seven_day_opus', 'seven_day_sonnet'])
  assert.deepEqual(usageColumns([w('seven_day', 11)]), { fiveHour: null, week: w('seven_day', 11), weekAll: [w('seven_day', 11)] }, 'Codex: weekly only')
  assert.deepEqual(usageColumns([]), { fiveHour: null, week: null, weekAll: [] })
})

test('barCell: the 5h window, with the weekly one only once it reaches the warning tier', () => {
  assert.deepEqual(barCell([w('five_hour', 12), w('seven_day', 42)], th()), { window: w('five_hour', 12), week: null })
  assert.deepEqual(
    barCell([w('five_hour', 40), w('seven_day', 30), w('seven_day_opus', 97)], th()),
    { window: w('five_hour', 40), week: w('seven_day_opus', 97) },
    'the most used weekly window',
  )
  // the warning tier comes from quota_warn_percent
  assert.equal(barCell([w('five_hour', 40), w('seven_day', 85)], th(90)).week, null)
  assert.deepEqual(barCell([w('seven_day', 11)], th()), { window: w('seven_day', 11), week: null }, 'Codex: weekly only')
  assert.deepEqual(barCell([w('seven_day', 91)], th()), { window: w('seven_day', 91), week: null }, 'no second mark for the shown window')
  assert.deepEqual(barCell([], th()), { window: null, week: null })
})

test('barRows: current first, hot accounts by %, the rest as listed, no numbers last', () => {
  const rows = [
    rowOf('claude:x', []),
    rowOf('claude:a', [w('five_hour', 12), w('seven_day', 42)]),
    rowOf('claude:b', [w('five_hour', 86), w('seven_day', 61)]),
    rowOf('claude:c', [w('five_hour', 40), w('seven_day', 97)]),
    rowOf('codex', [w('five_hour', 5), w('seven_day', 11)]),
    rowOf('codex:x', []),
    rowOf('codex:y', [w('five_hour', 1)]),
  ]
  assert.deepEqual(keysOf(barRows(rows, th(), 'claude:a')), ['claude:a', 'claude:c', 'claude:b', 'codex', 'codex:y', 'claude:x', 'codex:x'])
  // the warning tier comes from quota_warn_percent
  const late = [rowOf('codex', [w('five_hour', 5)]), rowOf('claude:b', [w('five_hour', 86)])]
  assert.deepEqual(keysOf(barRows(late, th(), null)), ['claude:b', 'codex'])
  assert.deepEqual(keysOf(barRows(late, th(90), null)), ['codex', 'claude:b'])
  // the current account leads even without numbers; an unknown key keeps the order
  assert.deepEqual(keysOf(barRows(rows, th(), 'codex:x')).slice(0, 2), ['codex:x', 'claude:c'])
  assert.deepEqual(keysOf(barRows(rows, th(), 'claude:zz')), ['claude:c', 'claude:b', 'claude:a', 'codex', 'codex:y', 'claude:x', 'codex:x'])
  assert.deepEqual(barRows([], th(), 'claude:a'), [])
})

test('currentUsageKey: the run pick, else the Project / global default, else the shared row', () => {
  const d = { engine: 'claude', claudeAccountId: 'p1' }
  assert.equal(currentUsageKey({ claude_account_id: 'a2' }, d), 'claude:a2')
  assert.equal(currentUsageKey({ claude_account_id: null }, d), 'claude:p1', 'no pick follows the Project')
  assert.equal(currentUsageKey(null, d), 'claude:p1', 'no run: the default')
  assert.equal(currentUsageKey(null, { engine: 'claude', claudeAccountId: '' }), 'claude', 'no account set up')
  assert.equal(currentUsageKey({ codex_account_id: 'x1' }, { engine: 'codex', claudeAccountId: 'p1' }), 'codex:x1')
  assert.equal(currentUsageKey({ codex_account_id: null }, { engine: 'codex', claudeAccountId: 'p1' }), 'codex', 'shared Codex login')
  assert.equal(currentUsageKey(null, { engine: 'codex', claudeAccountId: '' }), 'codex')
})

test('visibleUsageRings: keepKey keeps the Session account row without numbers', () => {
  const rows = [rowOf('claude', []), rowOf('codex', []), rowOf('codex:c', [])]
  assert.deepEqual(keysOf(visibleUsageRings(rows, 'codex')), ['codex', 'codex:c'])
})
