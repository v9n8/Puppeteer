// Unit tests for lib/backgroundTasks.ts (a live session whose turn is over but
// whose background tasks keep it running, BG-03).
// Run: node --test src/lib/__tests__/backgroundTasks.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { composerStatus, readSnapshot, taskLabel, turnBusy, turnJustEnded, waitsOnBackground } from '../backgroundTasks.ts'

const bash = { taskId: 'b1', taskType: 'local_bash', description: 'grep -n foo', subagentType: null }

test('readSnapshot keeps the tasks the backend names, in camelCase', () => {
  const got = readSnapshot({
    run_id: 'r1',
    turn_done: true,
    turn_ended_at: '2026-11-24T10:59:33Z',
    tasks: [
      { task_id: 'b1', task_type: 'local_bash', description: 'grep -n foo' },
      { task_id: 'a1', task_type: 'local_agent', description: 'review', subagent_type: 'general-purpose' },
      { task_type: 'local_bash', description: 'no id' },
      null,
    ],
  })
  assert.deepEqual(got, {
    turnDone: true,
    tasks: [bash, { taskId: 'a1', taskType: 'local_agent', description: 'review', subagentType: 'general-purpose' }],
  })
})

test('readSnapshot: nothing usable is null, a missing flag is false', () => {
  assert.equal(readSnapshot(null), null)
  assert.equal(readSnapshot('x'), null)
  assert.deepEqual(readSnapshot({}), { turnDone: false, tasks: [] })
  assert.deepEqual(readSnapshot({ turn_done: 'yes', tasks: 'no' }), { turnDone: false, tasks: [] })
})

test('turnBusy: running is a turn in progress unless the backend says it is over', () => {
  assert.equal(turnBusy('running', null), true)
  assert.equal(turnBusy('running', undefined), true)
  assert.equal(turnBusy('running', { turnDone: false, tasks: [bash] }), true)
  assert.equal(turnBusy('running', { turnDone: true, tasks: [bash] }), false)
  // Over, with no task left: the session is about to close; still not busy.
  assert.equal(turnBusy('running', { turnDone: true, tasks: [] }), false)
  for (const s of ['done', 'failed', 'cancelled', 'fetched', undefined]) {
    assert.equal(turnBusy(s, { turnDone: false, tasks: [] }), false)
  }
})

test('composerStatus: a live session whose turn is over reads as done', () => {
  assert.equal(composerStatus('running', null), 'running')
  assert.equal(composerStatus('running', { turnDone: false, tasks: [bash] }), 'running')
  assert.equal(composerStatus('running', { turnDone: true, tasks: [bash] }), 'done')
  for (const s of ['done', 'failed', 'cancelled', 'fetched']) assert.equal(composerStatus(s, { turnDone: true, tasks: [] }), s)
})

test('waitsOnBackground: live, turn over, and at least one task', () => {
  assert.equal(waitsOnBackground('running', { turnDone: true, tasks: [bash] }), true)
  assert.equal(waitsOnBackground('running', { turnDone: true, tasks: [] }), false)
  assert.equal(waitsOnBackground('running', { turnDone: false, tasks: [bash] }), false)
  assert.equal(waitsOnBackground('done', { turnDone: true, tasks: [bash] }), false)
  assert.equal(waitsOnBackground('running', null), false)
})

test('turnJustEnded fires once, on the flip to done', () => {
  const busy = { turnDone: false, tasks: [bash] }
  const done = { turnDone: true, tasks: [bash] }
  assert.equal(turnJustEnded(busy, done), true)
  assert.equal(turnJustEnded(null, done), true)
  assert.equal(turnJustEnded(done, done), false)
  assert.equal(turnJustEnded(done, { turnDone: true, tasks: [] }), false)
  assert.equal(turnJustEnded(done, busy), false)
  assert.equal(turnJustEnded(busy, null), false)
})

test('taskLabel: the description on one line, else the kind', () => {
  assert.equal(taskLabel({ ...bash, description: '  grep\n -n   foo ' }), 'grep -n foo')
  assert.equal(taskLabel({ ...bash, description: '', subagentType: 'Explore' }), 'Explore')
  assert.equal(taskLabel({ ...bash, description: '' }), 'local_bash')
  assert.equal(taskLabel({ taskId: 'x', taskType: '', description: '', subagentType: null }), 'x')
})
