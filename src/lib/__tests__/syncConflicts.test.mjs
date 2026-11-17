// Unit tests for lib/syncConflicts.ts (V3-47).
// Run: node --test src/lib/__tests__/syncConflicts.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { conflictPlace, scanMessage, scanOutcome, sortConflicts, truncateMiddle } from '../syncConflicts.ts'

test('conflictPlace names where the project copy lives', () => {
  assert.equal(conflictPlace({ kind: 'skill', engine: 'claude', block: null }), '.claude/skills')
  assert.equal(conflictPlace({ kind: 'skill', engine: 'codex', block: null }), '.agents/skills')
  assert.equal(conflictPlace({ kind: 'rule', engine: 'claude', block: null }), '.claude/rules')
  assert.equal(conflictPlace({ kind: 'rule', engine: 'codex', block: 'commit-rules' }), 'AGENTS.md')
  assert.equal(conflictPlace({ kind: 'rule', engine: 'codex', block: null }), '.agents/skills')
})

test('sortConflicts puts the newest detection first, then by name', () => {
  const list = [
    { name: 'b', engine: 'claude', detected_at: '2026-11-01T00:00:00Z' },
    { name: 'z', engine: 'claude', detected_at: '2026-11-03T00:00:00Z' },
    { name: 'a', engine: 'claude', detected_at: '2026-11-01T00:00:00Z' },
  ]
  assert.deepEqual(sortConflicts(list).map((c) => c.name), ['z', 'a', 'b'])
  assert.equal(list[0].name, 'b', 'input is not mutated')
})

test('truncateMiddle keeps both ends of a long path', () => {
  assert.equal(truncateMiddle('/a/b', 56), '/a/b')
  const cut = truncateMiddle('/Users/me/projects/app/.claude/skills/ui-review/SKILL.md', 24)
  assert.equal(cut.length, 24)
  assert.ok(cut.startsWith('/Users/me/'))
  assert.ok(cut.endsWith('SKILL.md'))
  assert.ok(cut.includes('…'))
})

const entry = (name) => ({ project_id: 'p', kind: 'skill', item_id: name, name, engine: 'claude', conflict_id: 'c-' + name })
const report = (over) => ({ projects: 1, checked: 3, open: [], opened: 0, closed: [], errors: [], ...over })
// Echoes the key and params so the test sees what the toast is built from.
const t = (key, params = {}) => `${key.split('.').pop()}(${Object.values(params).join('|')})`

test('scanOutcome splits still-open from now-matching, ignoring other close reasons', () => {
  const o = scanOutcome(report({
    open: [entry('commit-rules'), entry('commit-rules')],
    closed: [
      { ...entry('ui-review'), reason: 'in_sync' },
      { ...entry('old'), reason: 'not_applied' },
      { ...entry('dup'), reason: 'duplicate' },
    ],
    errors: [{ project_id: 'p', kind: null, name: null, engine: null, error: 'x' }],
  }))
  assert.deepEqual(o, { open: ['commit-rules'], synced: ['ui-review'], errors: 1 })
})

test('scanMessage reports what the scan found and nothing else', () => {
  assert.equal(
    scanMessage({ open: ['commit-rules'], synced: ['ui-review'], errors: 0 }, t),
    'scanStillOpen(1|commit-rules) scanSynced(ui-review)',
  )
  assert.equal(scanMessage({ open: [], synced: [], errors: 0 }, t), 'scanNoneOpen()')
  assert.equal(scanMessage({ open: [], synced: [], errors: 2 }, t), 'scanNoneOpen() scanErrors(2)')
})
