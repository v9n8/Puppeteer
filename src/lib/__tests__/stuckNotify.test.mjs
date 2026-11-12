// Unit tests for lib/stuckNotify.ts (HD-11: which watchdog notices raise a
// "session may be stuck" notification, once per alert).
// Run: node --test src/lib/__tests__/stuckNotify.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { STUCK_ALERT_KINDS, hasStuckAlert, isStuckAlert, newStuckAlerts, stuckAlertKey } from '../stuckNotify.ts'

const notice = (kind, key = kind, active) => (active === undefined ? { key, kind } : { key, kind, active })
const run = (runId, attention, projectId = 'p1') => ({ runId, projectId, attention })

test('only the two strong signals count as stuck', () => {
  assert.deepEqual([...STUCK_ALERT_KINDS], ['unresponsive', 'not_started'])
  assert.equal(isStuckAlert(notice('unresponsive')), true)
  assert.equal(isStuckAlert(notice('not_started')), true)
  for (const kind of ['quiet', 'long_tool', 'unmonitored', 'permission_waiting', 'input_stalled', '']) {
    assert.equal(isStuckAlert(notice(kind, kind === 'long_tool' ? 'tool:t1' : kind)), false, kind)
  }
})

test('a cleared notice is not stuck', () => {
  assert.equal(isStuckAlert(notice('unresponsive', 'unresponsive', false)), false)
  assert.equal(isStuckAlert(notice('unresponsive', 'unresponsive', true)), true)
})

test('the dock mark follows the run notices', () => {
  assert.equal(hasStuckAlert([]), false)
  assert.equal(hasStuckAlert([notice('quiet'), notice('long_tool', 'tool:t1')]), false)
  assert.equal(hasStuckAlert([notice('quiet'), notice('not_started')]), true)
})

test('a new strong alert is returned once, with its run', () => {
  const runs = [run('r1', [notice('quiet'), notice('unresponsive')]), run('r2', [notice('long_tool', 'tool:t1')], 'p2')]
  const first = newStuckAlerts(runs, new Set())
  assert.deepEqual(first.fresh, [{ runId: 'r1', projectId: 'p1', key: 'unresponsive', kind: 'unresponsive' }])
  assert.deepEqual([...first.notified], [stuckAlertKey('r1', 'unresponsive')])

  // The watchdog re-sends the same notice on a later tick: nothing new.
  const second = newStuckAlerts(runs, first.notified)
  assert.deepEqual(second.fresh, [])
  assert.deepEqual([...second.notified], [...first.notified])
})

test('quiet, long_tool and unmonitored never notify', () => {
  const runs = [run('r1', [notice('quiet'), notice('long_tool', 'tool:t1'), notice('unmonitored')])]
  const { fresh, notified } = newStuckAlerts(runs, new Set())
  assert.deepEqual(fresh, [])
  assert.equal(notified.size, 0)
})

test('cleared then raised again notifies again; a clear itself notifies nothing', () => {
  const raised = newStuckAlerts([run('r1', [notice('unresponsive')])], new Set())
  assert.equal(raised.fresh.length, 1)

  const cleared = newStuckAlerts([run('r1', [])], raised.notified)
  assert.deepEqual(cleared.fresh, [])
  assert.equal(cleared.notified.size, 0)

  const again = newStuckAlerts([run('r1', [notice('unresponsive')])], cleared.notified)
  assert.deepEqual(again.fresh, [{ runId: 'r1', projectId: 'p1', key: 'unresponsive', kind: 'unresponsive' }])
})

test('the same key on two runs is two alerts; both kinds on one run are two alerts', () => {
  const { fresh } = newStuckAlerts(
    [run('r1', [notice('unresponsive')]), run('r2', [notice('unresponsive'), notice('not_started')])],
    new Set([stuckAlertKey('r1', 'unresponsive')]),
  )
  assert.deepEqual(
    fresh.map((a) => `${a.runId}/${a.key}`),
    ['r2/unresponsive', 'r2/not_started'],
  )
})

test('a notice marked inactive in the list is skipped and forgotten', () => {
  const key = stuckAlertKey('r1', 'unresponsive')
  const { fresh, notified } = newStuckAlerts([run('r1', [notice('unresponsive', 'unresponsive', false)])], new Set([key]))
  assert.deepEqual(fresh, [])
  assert.equal(notified.has(key), false)
})
