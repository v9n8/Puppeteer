// Unit tests for lib/sessionHeaderMenu.ts: the blocks of the Session header's ⋯ menu.
// Run: node --test src/lib/__tests__/sessionHeaderMenu.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { sessionHeaderMenu } from '../sessionHeaderMenu.ts'

test('main window: no ⋯ menu', () => {
  assert.deepEqual(sessionHeaderMenu({ isSessionWindow: false }), [])
})

test('session window: the session actions, then delete last', () => {
  assert.deepEqual(sessionHeaderMenu({ isSessionWindow: true }), ['runActions', 'delete'])
})
