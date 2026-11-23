// Unit tests for lib/storageCleanup.ts (Stats › Storage cleanup, Settings › Storage).
// Run: node --test src/lib/__tests__/storageCleanup.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mergeCleanable, previewRows, formatLast, effectiveDays, isValidDays } from '../storageCleanup.ts'

const cat = (id, files, bytes, kept_items = 0, kept_bytes = 0) => ({ id, items: files, files, bytes, kept_items, kept_bytes })

test('a row the cleanup knows gets its share; a display-only row gets none', () => {
  const preview = { categories: [cat('app_logs', 2, 10), cat('worktrees', 1, 500, 3, 900)] }
  const merged = mergeCleanable(['app_logs', 'worktrees', 'database', 'claude_sessions'], preview)
  assert.deepEqual(merged.app_logs, { files: 2, bytes: 10, keptItems: 0, keptBytes: 0 })
  assert.deepEqual(merged.worktrees, { files: 1, bytes: 500, keptItems: 3, keptBytes: 900 })
  assert.equal(merged.database, null)
  assert.equal(merged.claude_sessions, null)
})

test('no preview yet: every cell stays empty', () => {
  assert.deepEqual(mergeCleanable(['app_logs'], null), { app_logs: null })
})

test('the preview lists the big kinds first and sums them', () => {
  const { rows, files, bytes } = previewRows([cat('app_logs', 1, 1), cat('mystery', 1, 1), cat('trash', 4, 100), cat('worktrees', 0, 0)])
  assert.deepEqual(rows.map((r) => r.id), ['trash', 'worktrees', 'app_logs', 'mystery'])
  assert.deepEqual([files, bytes], [6, 102])
})

test('the last cleanup is read, a broken one is ignored', () => {
  const last = formatLast({ at: '2026-11-23T10:00:00+00:00', trigger: 'auto', files: 3, bytes: 9, errors: 1 })
  assert.equal(last.at.toISOString(), '2026-11-23T10:00:00.000Z')
  assert.deepEqual([last.trigger, last.files, last.bytes, last.errors], ['auto', 3, 9, 1])
  assert.equal(formatLast(null), null)
  assert.equal(formatLast({ at: 'soon', trigger: 'auto', files: 0, bytes: 0, errors: 0 }), null)
})

test('N reads like the backend: 0 or 7–3650, anything else is 30', () => {
  for (const [value, want] of [['', 30], ['abc', 30], ['3', 30], ['0', 0], ['7', 7], [' 90 ', 90], ['3650', 3650], ['5000', 30], ['7.5', 30], [null, 30]]) {
    assert.equal(effectiveDays(value), want, String(value))
  }
  assert.equal(isValidDays('0'), true)
  assert.equal(isValidDays('30'), true)
  assert.equal(isValidDays('3'), false)
  assert.equal(isValidDays(''), false)
  assert.equal(isValidDays('5000'), false)
})
