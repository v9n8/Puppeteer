// Unit tests for lib/sessionInfoBar.ts (session info bar counts and context level).
// Run: node --test src/lib/__tests__/sessionInfoBar.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { connectionSummary, linkSummary, contextLevel } from '../sessionInfoBar.ts'

test('connectionSummary counts every group and lists only the non-empty ones', () => {
  const s = connectionSummary({ git: 2, servers: 1, aws: 1, azure: 0 })
  assert.equal(s.total, 4)
  assert.deepEqual(s.parts, [
    { kind: 'git', count: 2 },
    { kind: 'servers', count: 1 },
    { kind: 'aws', count: 1 },
  ])
})

test('connectionSummary of nothing has total 0 (no button)', () => {
  const s = connectionSummary({ git: 0, servers: 0, aws: 0, azure: 0 })
  assert.equal(s.total, 0)
  assert.deepEqual(s.parts, [])
})

test('linkSummary: one thread and two todos', () => {
  const on = { slack: true, todos: true, notes: true }
  const s = linkSummary({ slack: 1, todos: 2, notes: 0 }, on)
  assert.equal(s.total, 3)
  assert.deepEqual(s.parts, [{ kind: 'slack', count: 1 }, { kind: 'todos', count: 2 }])
})

test('linkSummary drops a group whose prop is off, and does not count it', () => {
  const s = linkSummary(
    { slack: 4, todos: 2, notes: 1 },
    { slack: false, todos: true, notes: true },
  )
  assert.equal(s.total, 3)
  assert.deepEqual(s.parts.map((p) => p.kind), ['todos', 'notes'])
})

test('linkSummary with all three off has total 0', () => {
  const off = { slack: false, todos: false, notes: false }
  assert.equal(linkSummary({ slack: 1, todos: 1, notes: 1 }, off).total, 0)
})

test('contextLevel switches at the warn percent and at 100', () => {
  assert.equal(contextLevel(41), 'ok')
  assert.equal(contextLevel(79), 'ok')
  assert.equal(contextLevel(80), 'warn')
  assert.equal(contextLevel(99), 'warn')
  assert.equal(contextLevel(100), 'over')
  assert.equal(contextLevel(70, 60), 'warn')
})
