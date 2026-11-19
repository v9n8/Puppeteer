// Unit tests for lib/sessionHeaderMenu.ts: the blocks of the Session header's ⋯ menu.
// Run: node --test src/lib/__tests__/sessionHeaderMenu.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { sessionHeaderMenu } from '../sessionHeaderMenu.ts'

test('main window: open in, view and settings of the project; no session actions or delete', () => {
  assert.deepEqual(
    sessionHeaderMenu({ isSessionWindow: false, hasProject: true }),
    ['openProjectIn', 'viewProject', 'projectSettings'],
  )
})

test('session window: open in, the session actions, project screens, then delete last', () => {
  assert.deepEqual(
    sessionHeaderMenu({ isSessionWindow: true, hasProject: true }),
    ['openProjectIn', 'runActions', 'viewProject', 'projectSettings', 'delete'],
  )
})

test('no project: the project screens drop out in both windows', () => {
  assert.deepEqual(sessionHeaderMenu({ isSessionWindow: false, hasProject: false }), ['openProjectIn'])
  assert.deepEqual(sessionHeaderMenu({ isSessionWindow: true, hasProject: false }), ['openProjectIn', 'runActions', 'delete'])
})
