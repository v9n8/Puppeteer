// Unit tests for lib/flagEditor.ts (V3-20).
// Run: node --test src/lib/__tests__/flagEditor.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { backendNameErrorKey, colorUsers, keyboardMove, moveId, restoreDisabled, sharedFlagSubline, validateFlagName } from '../flagEditor.ts'

const flag = (id, extra = {}) => ({ id, name: id, color: 'info', builtin_key: null, ...extra })

test('a name is refused when empty, too long or taken (any case)', () => {
  const others = [flag('Blocked')]
  assert.equal(validateFlagName('  ', others), 'sessionFlags.errors.nameRequired')
  assert.equal(validateFlagName('x'.repeat(25), others), 'sessionFlags.errors.nameTooLong')
  assert.equal(validateFlagName('blocked', others), 'sessionFlags.errors.nameTaken')
  assert.equal(validateFlagName('x'.repeat(24), others), null)
  assert.equal(validateFlagName('Chờ khách duyệt', others), null)
})

test('the backend refusals map to i18n keys; others stay as they are', () => {
  assert.equal(backendNameErrorKey("A flag named 'a' already exists"), 'sessionFlags.errors.nameTaken')
  assert.equal(backendNameErrorKey('The name is longer than 24 characters'), 'sessionFlags.errors.nameTooLong')
  assert.equal(backendNameErrorKey('The flag needs a name'), 'sessionFlags.errors.nameRequired')
  assert.equal(backendNameErrorKey('Project not found'), null)
})

test('the mapped messages are the ones the backend returns', () => {
  const source = readFileSync(new URL('../../../src-tauri/src/commands/session_flags.rs', import.meta.url), 'utf8')
  assert.ok(source.includes('The flag needs a name'))
  assert.ok(source.includes('The name is longer than'))
  assert.ok(source.includes('already exists'))
})

test('colorUsers lists the other flags with that color', () => {
  const flags = [flag('a', { color: 'success' }), flag('b', { color: 'success' }), flag('c')]
  assert.deepEqual(colorUsers(flags, 'success', 'a').map((f) => f.id), ['b'])
  assert.deepEqual(colorUsers(flags, 'success').map((f) => f.id), ['a', 'b'])
  assert.deepEqual(colorUsers(flags, 'warning'), [])
})

test('keyboardMove stays inside the list', () => {
  const ids = ['a', 'b', 'c']
  assert.equal(keyboardMove(ids, 1, -1), 0)
  assert.equal(keyboardMove(ids, 0, -1), 0)
  assert.equal(keyboardMove(ids, 2, 1), 2)
  assert.equal(keyboardMove(ids, 0, 1), 1)
})

test('moveId puts an item at its new slot', () => {
  assert.deepEqual(moveId(['a', 'b', 'c'], 2, 0), ['c', 'a', 'b'])
  assert.deepEqual(moveId(['a', 'b', 'c'], 0, 1), ['b', 'a', 'c'])
})

test('Restore is off only while every default is there', () => {
  const all = ['backlog', 'todo', 'in_progress', 'in_review', 'done'].map((k) => flag(k, { builtin_key: k }))
  assert.equal(restoreDisabled(all), true)
  assert.equal(restoreDisabled(all.slice(2)), false)
  assert.equal(restoreDisabled([]), false)
})

test('a shared flag line says how many Sessions use it, or that it is hidden', () => {
  const t = (key, p) => `${key}:${p.n}`
  assert.equal(sharedFlagSubline({ hidden: false, run_count: 3 }, t), 'sessionFlags.project.inUse:3')
  assert.equal(sharedFlagSubline({ hidden: true, run_count: 2 }, t), 'sessionFlags.project.hiddenSub:2')
})
