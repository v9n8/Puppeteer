// Unit tests for lib/memoryFiles.ts (Settings › AI › Memory).
// Run: node --test src/lib/__tests__/memoryFiles.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  GLOBAL_MEMORY_KINDS, PROJECT_MEMORY_KINDS, estimateTokens, isMemoryConflict, memoryTarget, utf8Bytes,
} from '../memoryFiles.ts'

test('targets carry a kind and a project id, never a path', () => {
  assert.deepEqual(memoryTarget('user', 'p1'), { kind: 'user' })
  assert.deepEqual(memoryTarget('codex_user', null), { kind: 'codex_user' })
  assert.deepEqual(memoryTarget('project_claude', 'p1'), { kind: 'project_claude', project_id: 'p1' })
  assert.deepEqual(memoryTarget('project_agents', 'p1'), { kind: 'project_agents', project_id: 'p1' })
  // A project file with no project picked has nothing to read yet.
  assert.equal(memoryTarget('project_claude', null), null)
  assert.equal(memoryTarget('project_agents', ''), null)
})

test('Settings edits the global files only, the Library a project\'s', () => {
  assert.deepEqual([...GLOBAL_MEMORY_KINDS], ['user', 'codex_user'])
  assert.deepEqual([...PROJECT_MEMORY_KINDS], ['project_claude', 'project_agents'])
})

test('token estimate is about four characters a token, rounded up', () => {
  assert.equal(estimateTokens(''), 0)
  assert.equal(estimateTokens('abcd'), 1)
  assert.equal(estimateTokens('abcde'), 2)
  assert.equal(estimateTokens('x'.repeat(4000)), 1000)
})

test('size is counted in UTF-8 bytes', () => {
  assert.equal(utf8Bytes('abc'), 3)
  assert.equal(utf8Bytes('Tiếng'), 7)
  assert.equal(utf8Bytes('日本'), 6)
})

test('a conflict is told apart from other save errors', () => {
  assert.equal(isMemoryConflict('memory_conflict: /p/CLAUDE.md changed on disk since it was opened'), true)
  assert.equal(isMemoryConflict('Project not found'), false)
})
