// Unit tests for lib/skillRows.ts (Skills sidebar rows, allocation matrix, SKILL.md frontmatter).
// Run: node --test src/lib/__tests__/skillRows.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { GROUP_NONE } from '../listGroups.ts'
import {
  FILTER_UNGROUPED,
  buildAllocationMatrix,
  buildSkillGroups,
  chipSwitchable,
  engineLabel,
  normalizeFrontmatter,
  parseFrontmatter,
  setFrontmatterField,
  validateFrontmatter,
} from '../skillRows.ts'

const skills = [
  { id: 'a', name: 'git-flow', description: 'Branching model', target: 'both' },
  { id: 'b', name: 'pdf', description: 'Read PDF files', target: 'claude' },
  { id: 'c', name: 'deploy', description: 'Ship to prod', target: 'codex' },
]
const groups = [
  { id: 'g2', name: 'Ops' },
  { id: 'g1', name: 'Dev' },
]
const itemGroups = { a: ['g1', 'g2'], c: ['g2', 'gone'] }
const opts = { search: '', group: '', ungroupedLabel: 'Ungrouped' }

test('buildSkillGroups follows group order, lists multi-group skills in each, ungrouped last', () => {
  const out = buildSkillGroups(skills, itemGroups, groups, opts)
  assert.deepEqual(out.map((g) => [g.key, g.label, g.rows.map((r) => r.id)]), [
    ['g2', 'Ops', ['a', 'c']],
    ['g1', 'Dev', ['a']],
    [GROUP_NONE, 'Ungrouped', ['b']],
  ])
  // Unknown group ids are ignored; the other groups ride along as badges.
  assert.deepEqual(out[0].rows[0].otherGroupIds, ['g1'])
  assert.deepEqual(out[0].rows[1].otherGroupIds, [])
})

test('buildSkillGroups filters by group, ungrouped and search', () => {
  assert.deepEqual(
    buildSkillGroups(skills, itemGroups, groups, { ...opts, group: 'g1' }).map((g) => g.key),
    ['g1'],
  )
  const ungrouped = buildSkillGroups(skills, itemGroups, groups, { ...opts, group: FILTER_UNGROUPED })
  assert.deepEqual(ungrouped.map((g) => g.rows.map((r) => r.id)), [['b']])
  const searched = buildSkillGroups(skills, itemGroups, groups, { ...opts, search: 'PROD ship' })
  assert.deepEqual(searched.map((g) => g.rows.map((r) => r.id)), [['c']])
})

test('parseFrontmatter reads and unquotes values', () => {
  const fm = parseFrontmatter('---\nname: x\ndescription: "say \\"hi\\": now"\n---\nbody')
  assert.deepEqual(fm, { name: 'x', description: 'say "hi": now', target: '' })
  assert.deepEqual(parseFrontmatter('no frontmatter'), { name: '', description: '', target: '' })
})

test('validateFrontmatter reports the first problem', () => {
  assert.equal(validateFrontmatter('---\nname: ok\ndescription: d\n---\n'), null)
  assert.equal(validateFrontmatter('---\nname: ok\n')?.key, 'errFrontmatterMissingClose')
  assert.equal(validateFrontmatter('---\ndescription: d\n---\n')?.key, 'errMissingName')
  assert.equal(validateFrontmatter('---\nname: not a slug\ndescription: d\n---\n')?.key, 'errNameSlug')
  assert.equal(validateFrontmatter('---\nname: ok\n---\n')?.key, 'errMissingDescription')
  assert.equal(validateFrontmatter('---\nname: ok\ndescription: d\ntarget: gpt\n---\n')?.key, 'errTargetInvalid')
  assert.deepEqual(validateFrontmatter('---\n!!bad\nname: ok\n---\n'), { key: 'errInvalidLine', params: { line: '!!bad' } })
})

test('normalizeFrontmatter joins wrapped values and quotes when needed', () => {
  const out = normalizeFrontmatter('---\nname: x\ndescription: first\n  second: part\n---\nbody')
  assert.equal(out.changed, true)
  assert.equal(out.content, '---\nname: x\ndescription: "first second: part"\n---\nbody')
  assert.equal(normalizeFrontmatter('---\nname: x\n---\n').changed, false)
})

test('setFrontmatterField replaces, appends and creates frontmatter', () => {
  const raw = '---\nname: x\ndescription: old\n  wrapped\ntarget: both\n---\n\n# Body'
  assert.equal(
    setFrontmatterField(raw, 'description', 'new: value'),
    '---\nname: x\ndescription: "new: value"\ntarget: both\n---\n\n# Body',
  )
  assert.equal(setFrontmatterField('---\nname: x\n---\n', 'target', 'codex'), '---\nname: x\ntarget: codex\n---\n')
  assert.equal(setFrontmatterField('# Body', 'name', 'y'), '---\nname: y\n---\n\n# Body')
})

const allocation = {
  engines: [
    { id: 'claude', project_dir: '.claude/skills', home_dir: '.claude/skills' },
    { id: 'codex', project_dir: '.agents/skills', home_dir: '.agents/skills' },
  ],
  projects: [{ id: 'p2', name: 'Two' }, { id: 'p1', name: 'One' }],
  chips: [
    { skill_id: 'a', project_id: 'p1', engine: 'codex', enabled: true, state: 'drifted' },
    { skill_id: 'a', project_id: 'p2', engine: 'claude', enabled: true, state: 'installed' },
    { skill_id: 'b', project_id: 'p2', engine: 'claude', enabled: false, state: 'git_managed' },
    { skill_id: 'gone', project_id: 'p1', engine: 'claude', enabled: true, state: 'shadowed' },
  ],
  global: null,
}

test('buildAllocationMatrix: a cell per project, a chip per engine, missing chips not installed', () => {
  const rows = buildAllocationMatrix(skills, allocation, '')
  assert.deepEqual(rows.map((r) => r.skill.id), ['a', 'b', 'c'])
  const brief = (row) => row.cells.map((c) => [c.projectId, c.chips.map((x) => `${x.engine}:${x.state}:${x.enabled}`)])
  assert.deepEqual(brief(rows[0]), [
    ['p2', ['claude:installed:true', 'codex:not_installed:false']],
    ['p1', ['claude:not_installed:false', 'codex:drifted:true']],
  ])
  assert.deepEqual(brief(rows[1])[0], ['p2', ['claude:git_managed:false', 'codex:not_installed:false']])
  assert.deepEqual(buildAllocationMatrix(skills, allocation, 'pdf').map((r) => r.skill.id), ['b'])
  assert.ok(rows.every((r) => r.global === null), 'no Global column while the scope is off')
})

test('buildAllocationMatrix: a Global chip per engine with a home folder', () => {
  const withGlobal = {
    ...allocation,
    engines: [...allocation.engines, { id: 'gemini', project_dir: '.gemini/skills', home_dir: null }],
    global: [
      { skill_id: 'a', engine: 'claude', state: 'installed' },
      { skill_id: 'b', engine: 'codex', state: 'taken' },
    ],
  }
  const rows = buildAllocationMatrix(skills, withGlobal, '')
  const brief = (row) => row.global.map((x) => `${x.engine}:${x.state}:${x.enabled}`)
  assert.deepEqual(brief(rows[0]), ['claude:installed:true', 'codex:not_installed:false'])
  assert.deepEqual(brief(rows[1]), ['claude:not_installed:false', 'codex:taken:false'])
  assert.deepEqual(brief(rows[2]), ['claude:not_installed:false', 'codex:not_installed:false'])
  assert.deepEqual(rows[0].cells[0].chips.map((c) => c.engine), ['claude', 'codex', 'gemini'])
})

test('buildAllocationMatrix follows the declared engine list (a new engine is one more chip)', () => {
  const withGemini = { ...allocation, engines: [...allocation.engines, { id: 'gemini', project_dir: '.gemini/skills' }] }
  const [row] = buildAllocationMatrix(skills.slice(0, 1), withGemini, '')
  assert.deepEqual(row.cells[0].chips.map((c) => c.engine), ['claude', 'codex', 'gemini'])
  assert.equal(engineLabel('gemini'), 'Gemini')
})

test('chipSwitchable: every state but git-managed and taken', () => {
  for (const state of ['installed', 'drifted', 'shadowed', 'not_installed']) {
    assert.equal(chipSwitchable({ engine: 'claude', enabled: false, state }), true, state)
  }
  assert.equal(chipSwitchable({ engine: 'claude', enabled: true, state: 'git_managed' }), false)
  assert.equal(chipSwitchable({ engine: 'claude', enabled: false, state: 'taken' }), false)
})
