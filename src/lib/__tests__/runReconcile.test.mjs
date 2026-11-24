// Unit tests for lib/runReconcile.ts (runs stuck on 'running' after a missed
// `run:done`, e.g. across a webview reload).
// Run: node --test src/lib/__tests__/runReconcile.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { resolveRunProjects, settleAfterTwoPasses, untrackedRunningRows } from '../runReconcile.ts'

test('settleAfterTwoPasses: a run missed once gets one pass of grace', () => {
  const stale = new Set()
  assert.deepEqual(settleAfterTwoPasses(['a'], new Set(), stale), [])
  assert.deepEqual([...stale], ['a'])
})

test('settleAfterTwoPasses: a run missed on two passes in a row is settled', () => {
  const stale = new Set()
  settleAfterTwoPasses(['a'], new Set(), stale)
  assert.deepEqual(settleAfterTwoPasses(['a'], new Set(), stale), ['a'])
  assert.equal(stale.size, 0)
})

test('settleAfterTwoPasses: a run the backend reports live is never settled', () => {
  const stale = new Set()
  assert.deepEqual(settleAfterTwoPasses(['a'], new Set(['a']), stale), [])
  assert.deepEqual(settleAfterTwoPasses(['a'], new Set(['a']), stale), [])
  assert.equal(stale.size, 0)
})

test('settleAfterTwoPasses: turning live between passes resets the grace', () => {
  const stale = new Set()
  settleAfterTwoPasses(['a'], new Set(), stale)
  settleAfterTwoPasses(['a'], new Set(['a']), stale)
  assert.deepEqual(settleAfterTwoPasses(['a'], new Set(), stale), [])
  assert.deepEqual([...stale], ['a'])
})

test('settleAfterTwoPasses: a run no longer shown as running leaves the stale set', () => {
  const stale = new Set(['gone', 'a'])
  assert.deepEqual(settleAfterTwoPasses(['a'], new Set(), stale), ['a'])
  assert.equal(stale.has('gone'), false)
})

test('untrackedRunningRows: running rows without a session only', () => {
  const rows = [
    { id: 'r1', status: 'running' },
    { id: 'r2', status: 'running' },
    { id: 'r3', status: 'done' },
  ]
  assert.deepEqual(untrackedRunningRows(rows, new Set(['r2'])), ['r1'])
  assert.deepEqual(untrackedRunningRows([], new Set()), [])
})

test('resolveRunProjects: runs already known load nothing', async () => {
  const loaded = []
  const known = new Map([['r1', 'p1']])
  const got = await resolveRunProjects(['r1'], (id) => known.get(id), ['p1', 'p2'], async (p) => { loaded.push(p) })
  assert.deepEqual(loaded, [])
  assert.deepEqual(got, [{ runId: 'r1', projectId: 'p1' }])
})

test('resolveRunProjects: loads projects one by one and stops once all are known', async () => {
  const loaded = []
  const known = new Map()
  const byProject = { p1: [], p2: [['r1', 'p2']], p3: [['r2', 'p3']], p4: [['x', 'p4']] }
  const got = await resolveRunProjects(['r1', 'r2'], (id) => known.get(id), ['p1', 'p2', 'p3', 'p4'], async (p) => {
    loaded.push(p)
    for (const [id, pid] of byProject[p]) known.set(id, pid)
  })
  assert.deepEqual(loaded, ['p1', 'p2', 'p3'])
  assert.deepEqual(got, [
    { runId: 'r1', projectId: 'p2' },
    { runId: 'r2', projectId: 'p3' },
  ])
})

test('resolveRunProjects: a run found in no project is left out', async () => {
  const got = await resolveRunProjects(['r1'], () => undefined, ['p1'], async () => {})
  assert.deepEqual(got, [])
})
