// Unit tests for lib/accountMenu.ts (the composer's account menu, UR-55).
// Run: node --test src/lib/__tests__/accountMenu.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { accountMenuRows } from '../accountMenu.ts'

const claude = [
  { value: 'a1', label: 'Work', usageKey: 'claude:a1' },
  { value: 'a2', label: 'Home', usageKey: 'claude:a2' },
  { value: 'a3', label: 'Old', usageKey: 'claude:a3' },
]
const marks = (rows) => rows.map((r) => `${r.value}:${r.isCurrent ? '✓' : ''}${r.isDefault ? '★' : ''}${r.expired ? '🔒' : ''}`)

test('a run with no pick: ✓ sits on the ★ row, clicking it changes nothing', () => {
  const rows = accountMenuRows(claude, 'a1', '')
  assert.deepEqual(marks(rows), ['a1:✓★', 'a2:', 'a3:'])
  assert.equal(rows[0].pick, null)
  assert.equal(rows[1].pick, 'a2')
})

test('a run on another account: clicking the ★ row saves "" (back to the default)', () => {
  const rows = accountMenuRows(claude, 'a1', 'a2')
  assert.deepEqual(marks(rows), ['a1:★', 'a2:✓', 'a3:'])
  assert.equal(rows[0].pick, '')
  assert.equal(rows[1].pick, null, 'the current row changes nothing')
  assert.equal(rows[2].pick, 'a3')
})

test('a run pinned to the default account itself: ✓★ on one row, no change on click', () => {
  const rows = accountMenuRows(claude, 'a1', 'a1')
  assert.deepEqual(marks(rows), ['a1:✓★', 'a2:', 'a3:'])
  assert.equal(rows[0].pick, null)
})

test('an expired login is locked: never a pick, even as the default', () => {
  const rows = accountMenuRows(claude, 'a3', 'a1', (key) => key === 'claude:a3')
  assert.deepEqual(marks(rows), ['a1:✓', 'a2:', 'a3:★🔒'])
  assert.equal(rows[2].pick, null)
})

test('Codex: the shared login ("") is the default and the run with no pick sits on it', () => {
  const codex = [
    { value: '', label: 'Shared Codex login', usageKey: 'codex' },
    { value: 'x1', label: 'Team', usageKey: 'codex:x1' },
  ]
  let rows = accountMenuRows(codex, '', '')
  assert.deepEqual(marks(rows), [':✓★', 'x1:'])
  assert.equal(rows[0].pick, null)
  rows = accountMenuRows(codex, '', 'x1')
  assert.deepEqual(marks(rows), [':★', 'x1:✓'])
  assert.equal(rows[0].pick, '')
})

test('the one login there is (value null) is current, not a default and not a choice', () => {
  const rows = accountMenuRows([{ value: null, label: 'Global ~/.claude', usageKey: 'claude' }], '', '')
  assert.deepEqual(rows.map((r) => [r.isCurrent, r.isDefault, r.pick]), [[true, false, null]])
})

test('no default account configured: no ★ and no ✓ until the run picks one', () => {
  assert.deepEqual(marks(accountMenuRows(claude, '', '')), ['a1:', 'a2:', 'a3:'])
})
