// Unit tests for lib/sessionPaths.ts (the Session row menu's folder + label rules).
// Run: node --test src/lib/__tests__/sessionPaths.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  WORKTREE_PROBE, platformOf, revealLabelKey, sessionFileRoot, sessionRootState, sessionWorkDir, treePathIn,
  worktreeReadable,
} from '../sessionPaths.ts'

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

test('the file root is the worktree while it exists, else the Project', () => {
  const worker = { worktree_path: '/wt/r1' }
  assert.equal(sessionFileRoot(worker, '/p', true), '/wt/r1')
  assert.equal(sessionFileRoot(worker, '/p', false), '/p')
  assert.equal(sessionFileRoot({ worktree_path: null }, '/p', true), '/p')
  assert.equal(sessionFileRoot({}, undefined, false), null)
})

test('a worktree root is used before the probe answers, but settled only after', () => {
  const worker = { worktree_path: '/wt/r1' }
  assert.deepEqual(sessionRootState(worker, '/p', undefined), { root: '/wt/r1', settled: false })
  assert.deepEqual(sessionRootState(worker, '/p', true), { root: '/wt/r1', settled: true })
  assert.deepEqual(sessionRootState(worker, '/p', false), { root: '/p', settled: true })
  // No worktree: the Project, nothing to wait for.
  assert.deepEqual(sessionRootState({}, '/p', undefined), { root: '/p', settled: true })
})

test('the worktree is readable only when the probe found its .git file', () => {
  assert.equal(WORKTREE_PROBE, '.git')
  assert.equal(worktreeReadable([true]), true)
  // Gone, or refused to this caller (a remote desktop and the app's data folder).
  assert.equal(worktreeReadable([false]), false)
  // The call failed (the root no longer resolves).
  assert.equal(worktreeReadable(null), false)
  assert.equal(worktreeReadable([]), false)
})

test('a Files tree path as the Session root names it', () => {
  // Same root: as the tree gives it.
  assert.equal(treePathIn('/p', 'src/a.ts', '/p', '/p'), 'src/a.ts')
  // The Project's tree in a worker reading from its worktree: the Project's file, absolute.
  assert.equal(treePathIn('/p', 'src/a.ts', '/wt/r1', '/p'), '/p/src/a.ts')
  assert.equal(treePathIn('/p/', 'a.ts', '/wt/r1', '/p/'), '/p/a.ts')
  // Another Project's tree: not ours.
  assert.equal(treePathIn('/q', 'src/a.ts', '/wt/r1', '/p'), null)
  assert.equal(treePathIn('/q', 'src/a.ts', '/p', '/p'), null)
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
