// Unit tests for lib/toolPermissions.ts (UX-31): the one-time copy of the
// "Always allow / deny" choices older versions kept in localStorage into the DB.
// Run: node --test src/lib/__tests__/toolPermissions.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  LEGACY_IMPORTED_KEY,
  LEGACY_STORAGE_KEY,
  groupToolPermissions,
  importLegacyToolPermissions,
  migrateToolName,
  parseLegacyToolPermissions,
} from '../toolPermissions.ts'

function memoryStorage(init = {}) {
  const data = new Map(Object.entries(init))
  return {
    data,
    getItem: (k) => (data.has(k) ? data.get(k) : null),
    setItem: (k, v) => void data.set(k, String(v)),
  }
}

const legacy = JSON.stringify({
  p1: { allow: ['Bash', 'mcp__gdrive__search', 'Bash'], deny: ['Write'] },
  p2: { allow: [], deny: ['mcp__gmail__send'] },
})

test('parse: every decision in order, old google tool names rewritten, duplicates dropped', () => {
  assert.deepEqual(parseLegacyToolPermissions(legacy), [
    { project_id: 'p1', tool_name: 'Bash', decision: 'allow' },
    { project_id: 'p1', tool_name: 'mcp__google__drive_search', decision: 'allow' },
    { project_id: 'p1', tool_name: 'Write', decision: 'deny' },
    { project_id: 'p2', tool_name: 'mcp__google__mail_send', decision: 'deny' },
  ])
})

test('parse: per-server google keys are dropped, not widened to the merged server', () => {
  assert.equal(migrateToolName('mcp__gdrive'), null)
  assert.equal(migrateToolName('mcp__gmail'), null)
  assert.equal(migrateToolName('mcp__github__create_issue'), 'mcp__github__create_issue')
  assert.deepEqual(parseLegacyToolPermissions(JSON.stringify({ p: { allow: ['mcp__gdrive'], deny: ['mcp__gmail'] } })), [])
})

test('parse: malformed input gives nothing or only the well-formed part, never throws', () => {
  for (const raw of [null, '', '{', 'null', '42', '"x"', '[]', '[{"allow":["Bash"]}]']) {
    assert.deepEqual(parseLegacyToolPermissions(raw), [], String(raw))
  }
  const mixed = JSON.stringify({
    good: { allow: ['Read', 7, null, '', { x: 1 }], deny: 'Write' },
    arr: ['Bash'],
    nul: null,
    str: 'Bash',
    both: { allow: ['Bash'], deny: ['Bash'] },
  })
  assert.deepEqual(parseLegacyToolPermissions(mixed), [
    { project_id: 'good', tool_name: 'Read', decision: 'allow' },
    // On both lists: denied, as the prompt check decides.
    { project_id: 'both', tool_name: 'Bash', decision: 'deny' },
  ])
})

test('import: sends once, marks done, and a second start sends nothing', async () => {
  const storage = memoryStorage({ [LEGACY_STORAGE_KEY]: legacy })
  const sent = []
  const send = async (entries) => void sent.push(entries)
  assert.equal(await importLegacyToolPermissions(storage, send), 'imported')
  assert.equal(sent.length, 1)
  assert.equal(sent[0].length, 4)
  assert.equal(storage.getItem(LEGACY_IMPORTED_KEY), '1')
  // The old copy is left for an older build.
  assert.equal(storage.getItem(LEGACY_STORAGE_KEY), legacy)
  assert.equal(await importLegacyToolPermissions(storage, send), 'already')
  assert.equal(sent.length, 1)
})

test('import: a failed send is not marked, so the next start retries', async () => {
  const storage = memoryStorage({ [LEGACY_STORAGE_KEY]: legacy })
  const failing = async () => { throw new Error('backend down') }
  assert.equal(await importLegacyToolPermissions(storage, failing), 'failed')
  assert.equal(storage.getItem(LEGACY_IMPORTED_KEY), null)
  let calls = 0
  assert.equal(await importLegacyToolPermissions(storage, async () => void calls++), 'imported')
  assert.equal(calls, 1)
})

test('import: nothing or malformed data is marked done without a call', async () => {
  for (const init of [{}, { [LEGACY_STORAGE_KEY]: '{not json' }, { [LEGACY_STORAGE_KEY]: '{"p":{"allow":[1]}}' }]) {
    const storage = memoryStorage(init)
    let calls = 0
    assert.equal(await importLegacyToolPermissions(storage, async () => void calls++), 'nothing')
    assert.equal(calls, 0)
    assert.equal(storage.getItem(LEGACY_IMPORTED_KEY), '1')
  }
  // No storage, or storage that throws: nothing to copy, no crash.
  assert.equal(await importLegacyToolPermissions(null, async () => assert.fail('no call')), 'nothing')
  const throwing = { getItem: () => { throw new Error('denied') }, setItem: () => { throw new Error('denied') } }
  assert.equal(await importLegacyToolPermissions(throwing, async () => assert.fail('no call')), 'nothing')
})

test('group: rows per project in their order, allow and deny apart', () => {
  const row = (project_id, tool_name, decision) => ({ project_id, tool_name, decision, created_at: 't' })
  assert.deepEqual(
    groupToolPermissions([row('p1', 'Bash', 'allow'), row('p1', 'Write', 'deny'), row('p1', 'Read', 'allow'), row('p2', 'Edit', 'deny')]),
    { p1: { allow: ['Bash', 'Read'], deny: ['Write'] }, p2: { allow: [], deny: ['Edit'] } },
  )
  assert.deepEqual(groupToolPermissions([]), {})
})
