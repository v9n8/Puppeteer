// Unit tests for lib/projectList.ts (Projects page list + selection, UX-49).
// Run: node --test src/lib/__tests__/projectList.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { filterProjects, selectedProjectId, projectTab, projectPlace, PROJECT_TABS } from '../projectList.ts'

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

test('the tabs are the five settings groups (UX-50)', () => {
  assert.deepEqual([...PROJECT_TABS], ['overview', 'defaults', 'connections', 'library', 'servers'])
})

test('an unknown tab opens the overview', () => {
  assert.equal(projectTab('overview'), 'overview')
  assert.equal(projectTab('library'), 'library')
  assert.equal(projectTab('nope'), 'overview')
  assert.equal(projectTab(undefined), 'overview')
  assert.equal(projectTab('toString'), 'overview')
})

test('no tab: the tab of the requested section, the old ids included', () => {
  assert.equal(projectTab(undefined, 'board'), 'overview')
  assert.equal(projectTab(undefined, 'aws'), 'connections')
  assert.equal(projectTab(undefined, 'claude'), 'defaults')
  assert.equal(projectTab('nope', 'tools'), 'servers')
  assert.equal(projectTab(undefined, 'nope'), 'overview')
  // A named tab wins over the section's own tab.
  assert.equal(projectTab('library', 'aws'), 'library')
})

test('projectPlace: tab and section from the query', () => {
  assert.deepEqual(projectPlace('connections', 'gitlab'), { tab: 'connections', section: 'gitlab' })
  assert.deepEqual(projectPlace(undefined, 'claude'), { tab: 'defaults', section: 'session' })
  assert.deepEqual(projectPlace('servers', undefined), { tab: 'servers', section: null })
  assert.deepEqual(projectPlace(undefined, undefined), { tab: 'overview', section: null })
})
