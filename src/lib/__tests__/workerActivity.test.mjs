// Unit tests for lib/workerActivity.ts (the conductor worker panel's live line, CO-06).
// Run: node --test src/lib/__tests__/workerActivity.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'

const { nextActivity, settledAt } = await import('../workerActivity.ts')

const assistant = (content, parent = null) => ({ type: 'assistant', parent_tool_use_id: parent, message: { content } })

test('a main-thread tool call names the tool; any event moves the time', () => {
  let a = nextActivity(undefined, assistant([{ type: 'text', text: 'x' }, { type: 'tool_use', name: 'Bash' }]), 1000)
  assert.deepEqual(a, { at: 1000, tool: 'Bash' })
  a = nextActivity(a, { type: 'user', message: { content: [{ type: 'tool_result' }] } }, 2000)
  assert.deepEqual(a, { at: 2000, tool: 'Bash' })
  a = nextActivity(a, { type: 'stream_event' }, 3000)
  assert.deepEqual(a, { at: 3000, tool: 'Bash' })
})

test('a sub-agent call keeps the main tool; a text-only reply clears it', () => {
  let a = nextActivity(undefined, assistant([{ type: 'tool_use', name: 'Agent' }]), 1)
  a = nextActivity(a, assistant([{ type: 'tool_use', name: 'Read' }], 'toolu_1'), 2)
  assert.equal(a.tool, 'Agent')
  a = nextActivity(a, assistant([{ type: 'text', text: 'Done' }]), 3)
  assert.deepEqual(a, { at: 3, tool: null })
})

test('odd payloads only move the time', () => {
  for (const e of [null, 'line', 42, { type: 'assistant' }, assistant('not an array')]) {
    assert.deepEqual(nextActivity({ at: 1, tool: 'Edit' }, e, 5), { at: 5, tool: 'Edit' })
  }
})

test('a settled worker is dated by its finish, not its turn start', () => {
  const run = { created_at: '2026-01-01T00:00:00Z', started_at: '2026-01-01T00:00:00Z' }
  assert.equal(settledAt({ ...run, last_activity_at: '2026-01-01T01:00:00Z', finished_at: '2026-01-01T02:00:00Z' }), '2026-01-01T02:00:00Z')
  assert.equal(settledAt({ ...run, last_activity_at: '2026-01-01T03:00:00Z', finished_at: '2026-01-01T02:00:00Z' }), '2026-01-01T03:00:00Z')
  assert.equal(settledAt({ ...run, finished_at: '2026-01-01T02:00:00Z' }), '2026-01-01T02:00:00Z')
  assert.equal(settledAt({ ...run, last_activity_at: null, finished_at: null }), '2026-01-01T00:00:00Z')
})
