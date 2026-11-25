// Unit tests for lib/permissionDecision.ts (the single permission decision writer).
// Run: node --test src/lib/__tests__/permissionDecision.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { decidePermission, answerPermission } from '../permissionDecision.ts'

function fakeStore({ queue, ok = true } = {}) {
  const calls = { allowed: [], denied: [], responded: [] }
  return {
    calls,
    get: () => ({ permissionQueue: queue }),
    rememberAllowedTool: (_r, tool) => calls.allowed.push(tool),
    rememberDeniedTool: (_r, tool) => calls.denied.push(tool),
    respondPermission: async (_r, req, decision, opts) => {
      calls.responded.push({ id: req.request_id, decision, opts })
      return ok
    },
  }
}
const req = (id, tool = 'Bash') => ({ request_id: id, tool_name: tool })

test('allow answers the head request once', async () => {
  const s = fakeStore({ queue: [req('a'), req('b')] })
  assert.equal(await decidePermission(s, 'r1', 'a', 'allow', false), true)
  assert.deepEqual(s.calls.responded, [{ id: 'a', decision: 'allow', opts: { remember: false } }])
  assert.deepEqual(s.calls.allowed, [])
})

test('a request that is not the head is dropped', async () => {
  const s = fakeStore({ queue: [req('a'), req('b')] })
  assert.equal(await decidePermission(s, 'r1', 'b', 'allow', false), false)
  assert.equal(await decidePermission(s, 'r1', 'zzz', 'deny', true), false)
  assert.equal(s.calls.responded.length, 0)
  assert.equal(s.calls.denied.length, 0)
})

test('an empty queue drops the decision', async () => {
  const s = fakeStore({ queue: [] })
  assert.equal(await decidePermission(s, 'r1', 'a', 'allow', false), false)
  assert.equal(s.calls.responded.length, 0)
})

test('Always remembers exactly once, allow and deny separately', async () => {
  const s = fakeStore({ queue: [req('a', 'Edit')] })
  await decidePermission(s, 'r1', 'a', 'allow', true)
  assert.deepEqual(s.calls.allowed, ['Edit'])
  assert.deepEqual(s.calls.denied, [])
  await decidePermission(s, 'r1', 'a', 'deny', true)
  assert.deepEqual(s.calls.allowed, ['Edit'])
  assert.deepEqual(s.calls.denied, ['Edit'])
})

test('a once-only prompt remembers nothing in the webview but still sends remember', async () => {
  const s = fakeStore({ queue: [{ ...req('a', 'gh'), once_only: true }] })
  assert.equal(await decidePermission(s, 'r1', 'a', 'allow', true), true)
  assert.deepEqual(s.calls.allowed, [])
  assert.deepEqual(s.calls.denied, [])
  assert.deepEqual(s.calls.responded, [{ id: 'a', decision: 'allow', opts: { remember: true } }])
})

test('a failed send returns false', async () => {
  const s = fakeStore({ queue: [req('a')], ok: false })
  assert.equal(await decidePermission(s, 'r1', 'a', 'allow', false), false)
})

test('answers go to the head question, stale ones are dropped', async () => {
  const s = fakeStore({ queue: [req('q', 'AskUserQuestion')] })
  assert.equal(await answerPermission(s, 'r1', 'old', { Q: 'x' }), false)
  assert.equal(await answerPermission(s, 'r1', 'q', { Q: 'x' }), true)
  assert.deepEqual(s.calls.responded, [{ id: 'q', decision: 'allow', opts: { answers: { Q: 'x' } } }])
})
