// Unit tests for lib/runFork.ts (fork / duplicate a session, UX-28).
// Run: node --test src/lib/__tests__/runFork.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { FORK_ERRORS, canForkRun, forkErrorKey, forkSource } from '../runFork.ts'

const run = (over = {}) => ({ id: 'r1', status: 'done', engine: 'claude', ...over })

test('canForkRun: a stopped Claude / Codex session that has run, not a worker', () => {
  for (const status of ['done', 'failed', 'cancelled']) assert.equal(canForkRun(run({ status })), true, status)
  assert.equal(canForkRun(run({ engine: 'codex' })), true)
  assert.equal(canForkRun(run({ status: 'running' })), false)
  assert.equal(canForkRun(run({ status: 'fetched' })), false)
  assert.equal(canForkRun(run({ engine: 'shell' })), false)
  assert.equal(canForkRun(run({ role: 'worker' })), false)
  assert.equal(canForkRun(run({ conductor_run_id: 'boss' })), false)
  // A conductor itself can be forked.
  assert.equal(canForkRun(run({ role: 'conductor' })), true)
})

test('forkErrorKey: each refusal of fork_run has a message, other errors show as is', () => {
  for (const code of FORK_ERRORS) assert.equal(forkErrorKey(code), `run.fork.errors.${code}`)
  assert.equal(forkErrorKey('disk full'), null)
  assert.equal(forkErrorKey(new Error('run_active')), null)
})

test('forkSource: the source while listed, gone when not, null for no fork', () => {
  const src = run({ id: 'src', title: 'Refactor' })
  const fork = run({ id: 'f', forked_from_run_id: 'src' })
  assert.equal(forkSource(fork, [src, fork]), src)
  assert.equal(forkSource(fork, [fork]), 'gone')
  assert.equal(forkSource(run(), [src]), null)
  assert.equal(forkSource(null, [src]), null)
})
