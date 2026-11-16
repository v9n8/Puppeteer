// Unit tests for lib/screenRoutes.ts (addresses of the Notes, Slack and Gantt screens).
// Run: node --test src/lib/__tests__/screenRoutes.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { ganttPath, notesPath, parseItemQuery, parseSlackQuery, slackPath } from '../screenRoutes.ts'

test('paths: empty parameters are left out', () => {
  assert.equal(notesPath(), '/notes')
  assert.equal(notesPath({ id: '', projectId: null }), '/notes')
  assert.equal(slackPath(), '/slack')
  assert.equal(slackPath({ add: false }), '/slack')
  assert.equal(ganttPath(), '/gantt')
  assert.equal(ganttPath(''), '/gantt')
})

test('paths: parameters become the query', () => {
  assert.equal(notesPath({ id: 'n1' }), '/notes?id=n1')
  assert.equal(notesPath({ projectId: 'p1', id: 'n1' }), '/notes?project=p1&id=n1')
  assert.equal(slackPath({ projectId: 'p1', id: 't1', add: true }), '/slack?project=p1&id=t1&add=1')
  assert.equal(ganttPath('p 1'), '/gantt?project=p+1')
})

test('parse: reads a query back, unknown or empty values become null', () => {
  assert.deepEqual(parseSlackQuery({ project: 'p1', id: ['t1', 'x'], add: '1' }), { projectId: 'p1', id: 't1', add: true })
  assert.deepEqual(parseSlackQuery({ project: '', id: 5 }), { projectId: null, id: null, add: false })
  assert.deepEqual(parseItemQuery({ id: 'n1', add: '1' }), { projectId: null, id: 'n1' })
})

test('parse undoes build', () => {
  const url = new URL(slackPath({ projectId: 'p1', id: 't1' }), 'http://x')
  assert.deepEqual(parseSlackQuery(Object.fromEntries(url.searchParams)), { projectId: 'p1', id: 't1', add: false })
})
