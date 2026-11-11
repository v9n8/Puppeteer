// Unit tests for lib/modalStack.ts (which open dialog takes Escape, and its z-index).
// Run: node --test src/lib/__tests__/modalStack.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { isTopModal, pushModal, removeModal } from '../modalStack.ts'

test('a dialog opened over another takes Escape and sits above it', () => {
  const settings = Symbol('settings')
  const confirm = Symbol('confirm')
  const zSettings = pushModal(settings)
  const zConfirm = pushModal(confirm)
  assert.ok(zConfirm > zSettings)
  assert.equal(isTopModal(confirm), true)
  assert.equal(isTopModal(settings), false)
  removeModal(confirm)
  assert.equal(isTopModal(settings), true)
  removeModal(settings)
  assert.equal(isTopModal(settings), false)
})

test('reopening moves a dialog to the top; removing an unknown one is a no-op', () => {
  const a = Symbol('a')
  const b = Symbol('b')
  pushModal(a)
  pushModal(b)
  pushModal(a)
  assert.equal(isTopModal(a), true)
  removeModal(Symbol('unknown'))
  removeModal(a)
  assert.equal(isTopModal(b), true)
  removeModal(b)
})
