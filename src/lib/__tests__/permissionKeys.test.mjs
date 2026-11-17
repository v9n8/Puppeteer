// Unit tests for lib/permissionKeys.ts (which focused elements swallow Enter).
// Run: node --test src/lib/__tests__/permissionKeys.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { isActivationTarget } from '../permissionKeys.ts'

const el = (tag, extra = {}) => ({
  closest: (sel) => (sel.split(/,\s*/).includes(tag) ? {} : null),
  ...extra,
})

test('button, summary, link and fields keep their own Enter', () => {
  for (const tag of ['button', 'summary', 'a', 'input', 'textarea', 'select']) {
    assert.equal(isActivationTarget(el(tag)), true, tag)
  }
  assert.equal(isActivationTarget(el('div', { isContentEditable: true })), true)
})

test('a child inside a button / summary counts too', () => {
  assert.equal(isActivationTarget({ closest: () => ({}) }), true)
})

test('the panel itself and plain nodes still mean Allow', () => {
  assert.equal(isActivationTarget(el('div')), false)
  assert.equal(isActivationTarget(null), false)
})
