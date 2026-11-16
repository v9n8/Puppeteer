// Unit tests for lib/sessionPaths.ts (the Session row menu's folder + label rules).
// Run: node --test src/lib/__tests__/sessionPaths.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { platformOf, revealLabelKey, sessionWorkDir } from '../sessionPaths.ts'

test('a worker with a worktree reports it, any other run reports the Project', () => {
  assert.equal(sessionWorkDir({ worktree_path: '/p/.wt/r1' }, '/p'), '/p/.wt/r1')
  assert.equal(sessionWorkDir({ worktree_path: null }, '/p'), '/p')
  assert.equal(sessionWorkDir({}, '/p'), '/p')
  assert.equal(sessionWorkDir({ worktree_path: '' }, '/p'), '/p')
})

test('no worktree and no Project yields nothing to copy', () => {
  assert.equal(sessionWorkDir({}, undefined), null)
  assert.equal(sessionWorkDir({}, null), null)
})

test('the reveal label key follows the platform', () => {
  assert.equal(revealLabelKey('mac'), 'run.revealDir.mac')
  assert.equal(revealLabelKey('windows'), 'run.revealDir.windows')
  assert.equal(revealLabelKey('linux'), 'run.revealDir.linux')
})

test('platform from the user agent', () => {
  assert.equal(platformOf('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)'), 'mac')
  assert.equal(platformOf('Mozilla/5.0 (Windows NT 10.0; Win64; x64)'), 'windows')
  assert.equal(platformOf('Mozilla/5.0 (X11; Linux x86_64)'), 'linux')
})
