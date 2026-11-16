// Unit tests for lib/validateAll.ts (Settings → Connections "Validate all").
// Run: node --test src/lib/__tests__/validateAll.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { summarizeChecks, nextRow } from '../validateAll.ts'
import { filterConnectionRows } from '../connectionTypes.ts'

test('summarizeChecks counts mixed results', () => {
  const sum = summarizeChecks([
    { outcome: 'ok' },
    { outcome: 'warn', messageKey: 'x' },
    { outcome: 'needsLogin', messageKey: 'y' },
    { outcome: 'error', message: 'boom' },
    { outcome: 'ok' },
  ])
  assert.deepEqual(sum, { ok: 3, needsLogin: 1, failed: 1 })
})

test('summarizeChecks ignores skipped rows and handles empty input', () => {
  assert.deepEqual(summarizeChecks([]), { ok: 0, needsLogin: 0, failed: 0 })
  assert.deepEqual(summarizeChecks([undefined, { outcome: 'ok' }]), { ok: 1, needsLogin: 0, failed: 0 })
})

test('nextRow walks the list and ends with null', () => {
  const rows = ['a', 'b']
  assert.equal(nextRow(rows, 0), 'a')
  assert.equal(nextRow(rows, 1), 'b')
  assert.equal(nextRow(rows, 2), null)
  assert.equal(nextRow([], 0), null)
})

test('the walk covers only the rows the filter shows', () => {
  const rows = [
    { type: 'github', id: '1' },
    { type: 'gitlab', id: '2' },
    { type: 'github', id: '3' },
  ]
  const shown = filterConnectionRows(rows, 'github')
  assert.deepEqual(shown.map((r) => r.id), ['1', '3'])
  assert.equal(nextRow(shown, 2), null)
})
