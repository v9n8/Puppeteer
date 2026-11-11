// Unit tests for lib/libraryDisk.ts (Library screen, UX-54).
// Run: node --test src/lib/__tests__/libraryDisk.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  PROJECT_ITEM_KINDS, deletesLink, isLibraryConflict, isLibraryExists, kindGroups, libraryPlace, libraryQuery,
  memoryItem, memoryKindOf,
} from '../libraryDisk.ts'

const item = (kind, name, engine = 'claude', description = '') => ({
  kind, name, engine, description, path: `/h/${engine}/${kind}/${name}`, status: 'file',
})

const items = [
  item('skill', 'pr-review', 'codex'),
  item('skill', 'pr-review', 'claude', 'Review a PR'),
  item('skill', 'deploy'),
  item('agent', 'tester', 'claude', 'Writes tests'),
]

test('kindGroups: the four kinds in order, empty ones kept, rows by name then engine', () => {
  const groups = kindGroups(items, '', (k) => k.toUpperCase())
  assert.deepEqual(groups.map((g) => [g.key, g.label, g.rows.length]), [
    ['skill', 'SKILL', 3], ['rule', 'RULE', 0], ['command', 'COMMAND', 0], ['agent', 'AGENT', 1],
  ])
  assert.deepEqual(groups[0].rows.map((r) => `${r.item.name}/${r.item.engine}`), [
    'deploy/claude', 'pr-review/claude', 'pr-review/codex',
  ])
  assert.equal(groups[0].rows[0].id, '/h/claude/skill/deploy')
})

test('kindGroups: a search keeps only the groups with a match, on name or description', () => {
  assert.deepEqual(kindGroups(items, 'TESTS', String).map((g) => [g.key, g.rows.length]), [['agent', 1]])
  assert.deepEqual(kindGroups(items, 'review', String).map((g) => [g.key, g.rows.length]), [['skill', 2]])
  assert.deepEqual(kindGroups(items, 'nothing', String), [])
})

test('kindGroups: a project\'s tab adds Memory after the four', () => {
  const memory = memoryItem('project_claude', '/p', { path: '/p/CLAUDE.md', link_target: null, exists: true, content: '', size: 0, hash: 'h', managed_blocks: [] })
  const groups = kindGroups([...items, memory], '', String, PROJECT_ITEM_KINDS)
  assert.deepEqual(groups.map((g) => [g.key, g.rows.length]), [
    ['skill', 3], ['rule', 0], ['command', 0], ['agent', 1], ['memory', 1],
  ])
  // Global keeps the four: a memory row isn't listed there.
  assert.deepEqual(kindGroups(items, '', String).map((g) => g.key), ['skill', 'rule', 'command', 'agent'])
})

test('memoryItem: a row per project file, there or not, or failing to read', () => {
  const file = (over) => ({ path: '/p/AGENTS.md', link_target: null, exists: true, content: 'x', size: 1, hash: 'h1', managed_blocks: [], ...over })

  const agents = memoryItem('project_agents', '/p', file({}))
  assert.equal(agents.kind, 'memory')
  assert.deepEqual([agents.engine, agents.name, agents.path, agents.hash, agents.status, agents.missing, agents.error], [
    'codex', 'AGENTS.md', '/p/AGENTS.md', 'h1', 'file', false, null,
  ])
  assert.equal(memoryKindOf(agents), 'project_agents')

  const missing = memoryItem('project_claude', '/p', file({ path: '/p/CLAUDE.md', exists: false, hash: null, content: '' }))
  assert.deepEqual([missing.engine, missing.name, missing.hash, missing.missing], ['claude', 'CLAUDE.md', null, true])
  assert.equal(memoryKindOf(missing), 'project_claude')

  const linked = memoryItem('project_agents', '/p', file({ link_target: '/p/CLAUDE.md' }))
  assert.deepEqual([linked.status, linked.link_target], ['symlink', '/p/CLAUDE.md'])

  // A failed read keeps its row, at the path the file would have.
  const failed = memoryItem('project_claude', '/p/', 'links outside the project')
  assert.deepEqual([failed.path, failed.error, failed.missing, failed.hash], [
    '/p/CLAUDE.md', 'links outside the project', false, null,
  ])
})

test('deletesLink: links are removed, files go to the trash', () => {
  assert.equal(deletesLink({ status: 'puppeteer_link' }), true)
  assert.equal(deletesLink({ status: 'symlink' }), true)
  assert.equal(deletesLink({ status: 'file' }), false)
  assert.equal(deletesLink({ status: 'git_tracked' }), false)
})

test('error prefixes from the backend', () => {
  assert.equal(isLibraryConflict('library_conflict: /a changed on disk'), true)
  assert.equal(isLibraryConflict('library_exists: /a already exists'), false)
  assert.equal(isLibraryExists('library_exists: /a already exists'), true)
  assert.equal(isLibraryExists(new Error('boom')), false)
})

test('libraryPlace / libraryQuery: the URL query round-trips', () => {
  assert.deepEqual(libraryPlace({}), { scope: 'global', projectId: null, path: null })
  assert.deepEqual(libraryPlace({ scope: 'project', project: 'p1', path: '/x/SKILL.md' }), {
    scope: 'project', projectId: 'p1', path: '/x/SKILL.md',
  })
  // A project link opens its tab without `scope`; Global drops the project.
  assert.deepEqual(libraryPlace({ project: 'p1' }), { scope: 'project', projectId: 'p1', path: null })
  assert.deepEqual(libraryPlace({ scope: 'global', project: 'p1' }), { scope: 'global', projectId: null, path: null })
  assert.deepEqual(libraryPlace({ scope: 'project', project: ['a'], path: '' }), { scope: 'project', projectId: null, path: null })

  const place = { scope: 'project', projectId: 'p1', path: '/x/a.md' }
  assert.deepEqual(libraryQuery(place), { scope: 'project', project: 'p1', path: '/x/a.md' })
  assert.deepEqual(libraryPlace(libraryQuery(place)), place)
  assert.deepEqual(libraryQuery({ scope: 'global', projectId: 'p1', path: null }), { scope: 'global' })
})
