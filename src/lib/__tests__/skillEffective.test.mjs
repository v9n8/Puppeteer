// Unit tests for lib/skillEffective.ts (the Skills screen's "What the agent sees" tab).
// Run: node --test src/lib/__tests__/skillEffective.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { effectiveCounts, effectiveSections } from '../skillEffective.ts'

const skill = (name, scope, extra = {}) => ({
  name,
  scope,
  origin: 'unmanaged',
  skill_id: null,
  shadowed: false,
  ...extra,
})

const claude = {
  engine: { id: 'claude', project_dir: '.claude/skills', home_dir: '.claude/skills' },
  home_wins: true,
  skills: [
    skill('review', 'global'),
    skill('review', 'project', { origin: 'library', skill_id: 'r', shadowed: true }),
    skill('deploy', 'project', { origin: 'git_managed' }),
    skill('Alpha', 'project', { origin: 'library', skill_id: 'a' }),
    skill('mine', 'global', { origin: 'library', skill_id: 'm' }),
  ],
}

const names = (sections) => sections.map((s) => [s.scope, s.folder, s.skills.map((k) => k.name)])

test('effectiveSections: Global then Project, loaded skills before shadowed ones', () => {
  assert.deepEqual(names(effectiveSections(claude)), [
    ['global', '~/.claude/skills', ['mine', 'review']],
    ['project', '.claude/skills', ['Alpha', 'deploy', 'review']],
  ])
  const project = effectiveSections(claude)[1].skills
  assert.equal(project.at(-1).shadowed, true)
})

test('effectiveSections: the search filters by name and keeps empty sections', () => {
  assert.deepEqual(names(effectiveSections(claude, '  ALP ')), [
    ['global', '~/.claude/skills', []],
    ['project', '.claude/skills', ['Alpha']],
  ])
})

test('effectiveSections: no Global section for an engine without a HOME folder', () => {
  const gemini = { engine: { id: 'gemini', project_dir: '.gemini/skills', home_dir: null }, home_wins: false, skills: [skill('x', 'project')] }
  assert.deepEqual(names(effectiveSections(gemini)), [['project', '.gemini/skills', ['x']]])
  assert.deepEqual(effectiveSections(undefined), [])
})

test('effectiveSections: leaves the backend list untouched', () => {
  const before = claude.skills.map((s) => s.name)
  effectiveSections(claude)
  assert.deepEqual(claude.skills.map((s) => s.name), before)
})

test('effectiveCounts: loaded and shadowed over both scopes', () => {
  assert.deepEqual(effectiveCounts(claude), { loaded: 4, shadowed: 1 })
  assert.deepEqual(effectiveCounts(undefined), { loaded: 0, shadowed: 0 })
})
