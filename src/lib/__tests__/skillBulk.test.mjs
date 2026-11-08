// Unit tests for lib/skillBulk.ts (bulk switch preview of the Skills Allocation matrix).
// Run: node --test src/lib/__tests__/skillBulk.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { bulkPlanByProject, bulkRequestReady, summarizeBulkPlan, toggleId } from '../skillBulk.ts'

const change = (project, skill, files) => ({
  project_id: project,
  project_name: project.toUpperCase(),
  skill_id: skill,
  skill_name: skill,
  engines: ['claude'],
  files,
})
const skip = (project, skill, reason) => ({
  project_id: project,
  project_name: project.toUpperCase(),
  skill_id: skill,
  skill_name: skill,
  engine: 'codex',
  reason,
  path: null,
})

const plan = {
  changes: [
    change('p1', 'a', [
      { action: 'create', path: '/p1/.claude/skills/a' },
      { action: 'remove', path: '/p1/.codex/skills/a' },
    ]),
    change('p2', 'a', [{ action: 'create', path: '/p2/.claude/skills/a' }]),
    change('p1', 'b', []),
  ],
  skipped: [skip('p3', 'a', 'occupied'), skip('p2', 'b', 'git_managed')],
  unchanged: 4,
}

test('summarizeBulkPlan counts files by action, flag-only changes included', () => {
  assert.deepEqual(summarizeBulkPlan(plan), { changes: 3, create: 2, remove: 1, skipped: 2, unchanged: 4 })
  assert.deepEqual(summarizeBulkPlan({ changes: [], skipped: [], unchanged: 0 }), {
    changes: 0,
    create: 0,
    remove: 0,
    skipped: 0,
    unchanged: 0,
  })
})

test('bulkPlanByProject keeps the backend order and puts skips with their project', () => {
  const sections = bulkPlanByProject(plan)
  assert.deepEqual(
    sections.map((s) => [s.projectId, s.projectName, s.changes.map((c) => c.skill_id), s.skipped.map((k) => k.reason)]),
    [
      ['p1', 'P1', ['a', 'b'], []],
      ['p2', 'P2', ['a'], ['git_managed']],
      ['p3', 'P3', [], ['occupied']],
    ],
  )
})

test('bulkRequestReady needs a skill or a group, a project and an engine', () => {
  const req = { skill_ids: ['a'], group_id: null, project_ids: ['p1'], engines: ['claude'], on: true }
  assert.equal(bulkRequestReady(req), true)
  assert.equal(bulkRequestReady({ ...req, skill_ids: [], group_id: 'g' }), true)
  assert.equal(bulkRequestReady({ ...req, skill_ids: [] }), false)
  assert.equal(bulkRequestReady({ ...req, project_ids: [] }), false)
  assert.equal(bulkRequestReady({ ...req, engines: [] }), false)
})

test('toggleId adds or drops an id and keeps the order of the full list', () => {
  const all = ['a', 'b', 'c']
  assert.deepEqual(toggleId(['c'], 'a', all), ['a', 'c'])
  assert.deepEqual(toggleId(['a', 'c'], 'a', all), ['c'])
  assert.deepEqual(toggleId([], 'gone', all), [])
})
