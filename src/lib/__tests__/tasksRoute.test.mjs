import test from 'node:test'
import assert from 'node:assert/strict'
import {
  focusRedirect, parseTasksQuery, resolveTaskView, tasksPath, tasksQuery, visibleTaskViews,
} from '../tasksRoute.ts'

test('tasksQuery leaves out what is not set', () => {
  assert.deepEqual(tasksQuery({}), {})
  assert.deepEqual(tasksQuery({ view: 'list', projectId: 'p1', id: 't9' }), { view: 'list', project: 'p1', id: 't9' })
  assert.deepEqual(tasksQuery({ view: 'focus', projectId: null, id: null }), { view: 'focus' })
})

test('parseTasksQuery reads a router query and ignores junk', () => {
  assert.deepEqual(parseTasksQuery({ view: 'list', project: 'p1', id: 't9' }), { view: 'list', projectId: 'p1', id: 't9' })
  assert.deepEqual(parseTasksQuery({ view: 'grid', project: '', id: [] }), { view: null, projectId: null, id: null })
  assert.deepEqual(parseTasksQuery({ view: ['focus', 'list'], id: ['a', 'b'] }), { view: 'focus', projectId: null, id: 'a' })
})

test('build then parse round-trips', () => {
  const loc = { view: 'list', projectId: '__none__', id: 'x' }
  assert.deepEqual(parseTasksQuery(tasksQuery(loc)), loc)
})

test('visibleTaskViews follows the two props', () => {
  assert.deepEqual(visibleTaskViews(true, true), ['focus', 'list'])
  assert.deepEqual(visibleTaskViews(false, true), ['list'])
  assert.deepEqual(visibleTaskViews(true, false), ['focus'])
  assert.deepEqual(visibleTaskViews(false, false), [])
})

test('resolveTaskView: asked, then saved, then the first on offer', () => {
  assert.equal(resolveTaskView('list', 'focus', ['focus', 'list']), 'list')
  assert.equal(resolveTaskView(null, 'list', ['focus', 'list']), 'list')
  assert.equal(resolveTaskView(null, null, ['focus', 'list']), 'focus')
  // Focus is off: the list opens whatever was asked or saved.
  assert.equal(resolveTaskView('focus', 'focus', ['list']), 'list')
  assert.equal(resolveTaskView(null, null, []), null)
})

test('/focus redirects to the Focus view and keeps project and id', () => {
  assert.deepEqual(focusRedirect({}), { path: '/tasks', query: { view: 'focus' } })
  assert.deepEqual(focusRedirect({ project: 'p', id: 't', view: 'list' }), {
    path: '/tasks',
    query: { view: 'focus', project: 'p', id: 't' },
  })
})

test('tasksPath', () => {
  assert.equal(tasksPath({}), '/tasks')
  assert.equal(tasksPath({ view: 'list', projectId: 'p1', id: 't 1' }), '/tasks?view=list&project=p1&id=t+1')
})
