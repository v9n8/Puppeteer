// Unit tests for lib/runningSessions.ts (rows of the Running screen's Session tab).
// Run: node --test src/lib/__tests__/runningSessions.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  filterRunning, finishedRunning, runningCounts, runningRows, sortRunning, stopTargets, toolArg,
} from '../runningSessions.ts'

const projects = [{ id: 'p1', name: 'Puppeteer' }, { id: 'p2', name: 'pms-api' }]
const live = (runId, projectId, o = {}) => ({
  runId, projectId, status: 'running', permissionQueue: [], notifyDone: false, endReason: null, stuck: false, entries: [], ...o,
})
const rec = (project_id, started_at, o = {}) => ({
  project_id, status: 'running', engine: 'claude', model: 'opus', started_at, created_at: started_at,
  title: null, run_type: 'session', ref_number: null, ...o,
})
const meta = (obj) => ({ get: (id) => obj[id] })
const perm = { tool_name: 'Bash', tool_input: { command: 'pnpm migrate' } }

test('toolArg: the recognisable field, else the first text', () => {
  assert.equal(toolArg({ command: 'ls', path: '/x' }), 'ls')
  assert.equal(toolArg({ n: 1, other: 'hi' }), 'hi')
  assert.equal(toolArg(null), '')
})

test('a Session waiting on a permission floats above older running ones', () => {
  const rows = runningRows(
    [live('a', 'p1'), live('b', 'p2', { permissionQueue: [perm] })],
    meta({ a: rec('p1', '2026-01-01T00:00:00Z'), b: rec('p2', '2026-01-02T00:00:00Z') }),
    projects,
  )
  const sorted = sortRunning(rows)
  assert.deepEqual(sorted.map((r) => r.runId), ['b', 'a'])
  assert.equal(sorted[0].waitingOn, 'user')
  assert.deepEqual(sorted[0].doing, { kind: 'permission', name: 'Bash', arg: 'pnpm migrate' })
  assert.equal(sorted[1].waitingOn, 'agent')
})

test('running Sessions sort longest first; the open tool call is "doing"', () => {
  const rows = runningRows(
    [
      live('new', 'p1'),
      live('old', 'p1', { entries: [{ kind: 'tool', name: 'Bash', input: { command: 'pnpm test' } }] }),
    ],
    meta({ new: rec('p1', '2026-01-02T00:00:00Z'), old: rec('p1', '2026-01-01T00:00:00Z') }),
    projects,
  )
  const sorted = sortRunning(rows)
  assert.deepEqual(sorted.map((r) => r.runId), ['old', 'new'])
  assert.deepEqual(sorted[0].doing, { kind: 'tool', name: 'Bash', arg: 'pnpm test' })
})

test('a conductor folds its two workers; a blocked worker makes it wait', () => {
  const rows = runningRows(
    [live('c', 'p1'), live('w1', 'p1'), live('w2', 'p1', { permissionQueue: [perm] })],
    meta({
      c: rec('p1', '2026-01-01T00:00:00Z', { role: 'conductor' }),
      w1: rec('p1', '2026-01-01T00:01:00Z', { conductor_run_id: 'c' }),
      w2: rec('p1', '2026-01-01T00:02:00Z', { conductor_run_id: 'c' }),
    }),
    projects,
  )
  assert.equal(rows.length, 1)
  assert.equal(rows[0].workers.length, 2)
  assert.equal(rows[0].status, 'wait')
  assert.equal(rows[0].pending, 1)
})

test('a conductor with only running workers waits on them', () => {
  const rows = runningRows(
    [live('c', 'p1'), live('w1', 'p1'), live('w2', 'p1')],
    meta({
      c: rec('p1', '2026-01-01T00:00:00Z', { role: 'conductor' }),
      w1: rec('p1', '2026-01-01T00:01:00Z', { conductor_run_id: 'c' }),
      w2: rec('p1', '2026-01-01T00:02:00Z', { conductor_run_id: 'c' }),
    }),
    projects,
  )
  assert.equal(rows[0].waitingOn, 'worker')
  assert.deepEqual(rows[0].doing, { kind: 'workers', name: '', arg: '2' })
})

test('a conductor that ended its turn is rebuilt from its record; finished workers are dropped', () => {
  const rows = runningRows(
    [
      live('c', 'p1', { status: 'done' }),
      live('w1', 'p1'),
      live('w2', 'p1', { status: 'done', notifyDone: true }),
    ],
    meta({
      c: rec('p1', '2026-01-01T00:00:00Z', { role: 'conductor', status: 'done' }),
      w1: rec('p1', '2026-01-01T00:01:00Z', { conductor_run_id: 'c' }),
      w2: rec('p1', '2026-01-01T00:02:00Z', { conductor_run_id: 'c' }),
    }),
    projects,
  )
  assert.equal(rows.length, 1)
  assert.equal(rows[0].status, 'run')
  assert.deepEqual(rows[0].workers.map((w) => w.runId), ['w1'])
})

test('finished and unseen Sessions form their own group; a worker never does', () => {
  const rows = runningRows(
    [
      live('d', 'p2', { status: 'done', notifyDone: true }),
      live('x', 'p2', { status: 'failed', notifyDone: true }),
      live('w', 'p2', { status: 'done', notifyDone: true }),
      live('seen', 'p2', { status: 'done' }),
      live('r', 'p1'),
    ],
    meta({
      d: rec('p2', '2026-01-01T00:00:00Z'), x: rec('p2', '2026-01-01T00:00:00Z'),
      w: rec('p2', '2026-01-01T00:00:00Z', { conductor_run_id: 'zzz' }),
      seen: rec('p2', '2026-01-01T00:00:00Z'), r: rec('p1', '2026-01-01T00:00:00Z'),
    }),
    projects,
  )
  const all = { status: 'all', projectId: null }
  assert.deepEqual(finishedRunning(rows, all).map((r) => r.runId).sort(), ['d', 'x'])
  assert.equal(finishedRunning(rows, all).find((r) => r.runId === 'x').status, 'err')
  assert.deepEqual(filterRunning(rows, all).map((r) => r.runId), ['r'])
  assert.deepEqual(finishedRunning(rows, { status: 'wait', projectId: null }), [])
})

test('filter by status and Project; counts follow the Project filter', () => {
  const rows = runningRows(
    [live('a', 'p1', { permissionQueue: [perm] }), live('b', 'p1'), live('c', 'p2')],
    meta({ a: rec('p1', '2026-01-01T00:00:00Z'), b: rec('p1', '2026-01-01T00:00:00Z'), c: rec('p2', '2026-01-01T00:00:00Z') }),
    projects,
  )
  assert.deepEqual(runningCounts(rows, null), { all: 3, wait: 1, run: 2 })
  assert.deepEqual(runningCounts(rows, 'p2'), { all: 1, wait: 0, run: 1 })
  assert.deepEqual(filterRunning(rows, { status: 'wait', projectId: null }).map((r) => r.runId), ['a'])
  assert.deepEqual(filterRunning(rows, { status: 'run', projectId: 'p1' }).map((r) => r.runId), ['b'])
})

test('stopTargets: selected Sessions plus the running workers of their conductors', () => {
  const rows = runningRows(
    [live('c', 'p1'), live('w1', 'p1'), live('w2', 'p1'), live('s', 'p2'), live('other', 'p2')],
    meta({
      c: rec('p1', '2026-01-01T00:00:00Z', { role: 'conductor' }),
      w1: rec('p1', '2026-01-01T00:01:00Z', { conductor_run_id: 'c' }),
      w2: rec('p1', '2026-01-01T00:02:00Z', { conductor_run_id: 'c' }),
      s: rec('p2', '2026-01-01T00:00:00Z'), other: rec('p2', '2026-01-01T00:00:00Z'),
    }),
    projects,
  )
  const t = stopTargets(['c', 's', 'gone'], rows)
  assert.deepEqual(t.sessions.map((r) => r.runId).sort(), ['c', 's'])
  assert.deepEqual(t.workers.map((r) => r.runId), ['w1', 'w2'])
  assert.deepEqual(t.ids.sort(), ['c', 's', 'w1', 'w2'])
})
