// Unit tests for lib/usageRing.ts (usage rings on the session status bar).
// Run: node --test src/lib/__tests__/usageRing.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  usageWindows, ringWindow, ringSeverity, windowSeverity, parsePercentSetting, resetCountdown, usageSummary,
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
