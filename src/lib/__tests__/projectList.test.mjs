// Unit tests for lib/projectList.ts (Projects page list + selection, UX-49).
// Run: node --test src/lib/__tests__/projectList.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { filterProjects, selectedProjectId, projectTab } from '../projectList.ts'

const projects = [
  { id: 'a', name: 'Puppeteer', path: '/Users/me/repos/puppeteer', hidden: false },
  { id: 'b', name: 'Shop API', path: '/Users/me/work/shop-api', hidden: true },
  { id: 'c', name: 'Docs', path: '/Users/me/work/docs-site', hidden: false },
]

test('search matches the name or the path, case-insensitive, every term', () => {
  assert.deepEqual(filterProjects(projects, 'pupp').map((p) => p.id), ['a'])
  assert.deepEqual(filterProjects(projects, 'WORK').map((p) => p.id), ['b', 'c'])
  assert.deepEqual(filterProjects(projects, 'work docs').map((p) => p.id), ['c'])
  assert.deepEqual(filterProjects(projects, 'nothing').map((p) => p.id), [])
})

test('an empty search keeps every project in list order', () => {
  assert.deepEqual(filterProjects(projects, '  ').map((p) => p.id), ['a', 'b', 'c'])
})

test('the requested project is shown while it exists, even when hidden from the list', () => {
  const listed = projects.filter((p) => !p.hidden)
  assert.equal(selectedProjectId(projects, listed, 'c'), 'c')
  assert.equal(selectedProjectId(projects, listed, 'b'), 'b')
})

test('no or an unknown request falls back to the first listed project', () => {
  const listed = projects.filter((p) => !p.hidden)
  assert.equal(selectedProjectId(projects, listed, undefined), 'a')
  assert.equal(selectedProjectId(projects, listed, 'gone'), 'a')
  assert.equal(selectedProjectId(projects, listed, ['a']), 'a')
  assert.equal(selectedProjectId(projects, [], 'gone'), null)
  assert.equal(selectedProjectId([], [], undefined), null)
})

test('an unknown tab opens the overview', () => {
  assert.equal(projectTab('overview'), 'overview')
  assert.equal(projectTab('nope'), 'overview')
  assert.equal(projectTab(undefined), 'overview')
})
