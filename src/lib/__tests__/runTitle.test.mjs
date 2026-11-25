// Unit tests for lib/runTitle.ts (a session's automatic short name).
// Run: node --test src/lib/__tests__/runTitle.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { applyRunTitle, EVT_RUN_TITLE } from '../runTitle.ts'

test('the event name matches the backend', () => {
  assert.equal(EVT_RUN_TITLE, 'run:title')
})

test('applyRunTitle: the on-screen row and the cached meta both take the name', () => {
  const runs = [{ id: 'a', title: 'Explain the usage guard hook in detail' }, { id: 'b', title: 'Other' }]
  const meta = new Map([['a', { id: 'a', title: 'Explain the usage guard hook in detail' }]])
  applyRunTitle(runs, meta, 'a', 'Usage guard hook')
  assert.equal(runs[0].title, 'Usage guard hook')
  assert.equal(meta.get('a').title, 'Usage guard hook')
  assert.equal(runs[1].title, 'Other')
})

test('applyRunTitle: a run that is not loaded changes nothing and does not throw', () => {
  const runs = [{ id: 'a', title: 'Mine' }]
  const meta = new Map([['a', runs[0]]])
  assert.doesNotThrow(() => applyRunTitle(runs, meta, 'elsewhere', 'Name'))
  assert.deepEqual(runs, [{ id: 'a', title: 'Mine' }])
  assert.equal(meta.size, 1)
})

test('applyRunTitle: a run only in runMeta (another project) is updated there', () => {
  const meta = new Map([['x', { id: 'x', title: 'Provisional' }]])
  applyRunTitle([], meta, 'x', 'Short name')
  assert.equal(meta.get('x').title, 'Short name')
})
