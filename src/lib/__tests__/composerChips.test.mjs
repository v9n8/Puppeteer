// Unit tests for lib/composerChips.ts (UX-26): the defaults and choices of the
// composer's permission mode / effort / response style chips.
// Run: node --test src/lib/__tests__/composerChips.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  composerDefaults, effortForLaunch, effectiveEffort, inheritedEffort, inheritedModel, inheritedStyle, composerStyleChoices,
} from '../composerChips.ts'
import { effortLevelsFor } from '../engineOptions.ts'

const none = { engine: 'claude', models: { claude: '', codex: '' }, permissionMode: 'default', effort: '', style: '', styles: [] }

test('defaults: global settings, with unset or invalid values normalised', () => {
  assert.deepEqual(composerDefaults({ settings: null }), none)
  assert.deepEqual(
    composerDefaults({ settings: { default_permission_mode: 'plan', default_effort: ' high ', default_style: 'concise' } }),
    { ...none, permissionMode: 'plan', effort: 'high', style: 'concise', styles: ['concise'] },
  )
  assert.deepEqual(
    composerDefaults({ settings: { default_permission_mode: 'nonsense', default_effort: 'turbo', default_style: '  ', default_engine: 'gemini' } }),
    none,
  )
})

const global = {
  default_engine: 'claude', claude_model: 'claude-opus-4-8', codex_model: 'gpt-5.4',
  default_permission_mode: 'acceptEdits', default_effort: 'low', default_style: 'concise',
}

test('defaults (UX-19): the project\'s value, else the global one, field by field', () => {
  assert.deepEqual(composerDefaults({ settings: global, project: null }), {
    engine: 'claude', models: { claude: 'claude-opus-4-8', codex: 'gpt-5.4' },
    permissionMode: 'acceptEdits', effort: 'low', style: 'concise', styles: ['concise'],
  })
  assert.deepEqual(
    composerDefaults({
      settings: global,
      project: { engine: 'codex', model_codex: 'gpt-5.5', permission_mode: 'plan', effort: 'high', style: 'my-style' },
    }),
    {
      engine: 'codex', models: { claude: 'claude-opus-4-8', codex: 'gpt-5.5' },
      permissionMode: 'plan', effort: 'high', style: 'my-style', styles: ['my-style', 'concise'],
    },
  )
  // Blank or invalid project values follow the global setting.
  assert.deepEqual(
    composerDefaults({
      settings: global,
      project: { engine: 'gemini', model_claude: ' ', model_codex: null, permission_mode: 'yolo', effort: 'turbo', style: '' },
    }),
    composerDefaults({ settings: global }),
  )
})

test('defaults (UX-19): a session keeps the effort and model it ran with', () => {
  const d = composerDefaults({ settings: global, project: { model_codex: 'gpt-5.5', effort: 'high' } })
  assert.equal(inheritedEffort('max', d.effort), 'max')
  assert.equal(inheritedEffort(null, d.effort), 'high')
  assert.equal(inheritedEffort('turbo', d.effort), 'high')
  assert.equal(inheritedModel({ engine: 'codex', model: 'gpt-5.3-codex' }, 'codex', d), 'gpt-5.3-codex')
  assert.equal(inheritedModel({ engine: 'codex', model: null }, 'codex', d), 'gpt-5.5')
  // The run is on the other engine (a handoff): that engine's default.
  assert.equal(inheritedModel({ engine: 'claude', model: 'claude-x' }, 'codex', d), 'gpt-5.5')
  assert.equal(inheritedModel(null, 'claude', d), 'claude-opus-4-8')
})

test('effort: a pick is sent only when the next model takes it', () => {
  const claude = effortLevelsFor('claude', 'claude-opus-5-5')
  const sonnet46 = effortLevelsFor('claude', 'claude-sonnet-4-6') // no xhigh
  const haiku = effortLevelsFor('claude', 'claude-haiku-4-5') // no effort at all
  assert.equal(effortForLaunch('xhigh', claude), 'xhigh')
  assert.equal(effortForLaunch('xhigh', sonnet46), undefined)
  assert.equal(effortForLaunch('high', haiku), undefined)
  assert.equal(effortForLaunch('', claude), undefined)
  assert.equal(effortForLaunch('max', effortLevelsFor('codex', 'gpt-5.5')), undefined)
})

test('effort: the shown value is the pick, else a default the model takes, else engine decides', () => {
  const claude = effortLevelsFor('claude', '')
  const sonnet46 = effortLevelsFor('claude', 'claude-sonnet-4-6')
  assert.equal(effectiveEffort('low', claude, 'high'), 'low')
  assert.equal(effectiveEffort('', claude, 'high'), 'high')
  assert.equal(effectiveEffort('xhigh', sonnet46, 'medium'), 'medium')
  assert.equal(effectiveEffort('', sonnet46, 'xhigh'), '')
  assert.equal(effectiveEffort('', claude, ''), '')
})

test('style: the run keeps its stored style, else the default, else none', () => {
  assert.equal(inheritedStyle('concise', 'code_review', null), 'concise')
  assert.equal(inheritedStyle(null, 'code_review', null), 'code_review')
  assert.equal(inheritedStyle('  ', '', null), 'default')
  assert.equal(inheritedStyle('default', 'concise', null), 'default')
})

test('style: references that no longer resolve are skipped, like the backend', () => {
  const known = (id) => id === 'my-1'
  assert.equal(inheritedStyle('gone', 'my-1', known), 'my-1')
  assert.equal(inheritedStyle('gone', 'also-gone', known), 'default')
  // A sample is a row (UX-36): deleted, it is skipped too.
  assert.equal(inheritedStyle('explain_junior', 'my-1', known), 'my-1')
  // Before the list loads, every reference counts.
  assert.equal(inheritedStyle('gone', 'my-1', null), 'gone')
})

test('style (UX-19): a deleted project default falls through to the global one', () => {
  const known = (id) => id === 'my-1'
  assert.equal(inheritedStyle(null, ['project-gone', 'my-1'], known), 'my-1')
  assert.equal(inheritedStyle(null, ['my-1', 'concise'], known), 'my-1')
  assert.equal(inheritedStyle('concise', ['my-1'], (id) => id === 'concise'), 'concise')
  assert.equal(inheritedStyle(null, [], known), 'default')
})

const styles = [
  { id: 'concise', project_id: null, name: 'Concise', description: 'sample' },
  { id: 'g1', project_id: null, name: 'Mine', description: 'global' },
  { id: 'p1', project_id: 'proj-a', name: 'Project A', description: '' },
  { id: 'p2', project_id: 'proj-b', name: 'Project B', description: '' },
]

test('style choices: none, the global styles (samples included), then this project\'s', () => {
  const choices = composerStyleChoices(styles, 'proj-a', 'concise')
  assert.deepEqual(choices.map((c) => [c.kind, c.value]), [
    ['none', 'default'],
    ['mine', 'concise'],
    ['mine', 'g1'],
    ['project', 'p1'],
  ])
  assert.deepEqual(choices.at(-1), { kind: 'project', value: 'p1', name: 'Project A', description: '' })
})

test('style choices: another project\'s or a deleted style stays listed as missing', () => {
  assert.deepEqual(composerStyleChoices(styles, 'proj-a', 'p2').at(-1), { kind: 'missing', value: 'p2' })
  assert.deepEqual(composerStyleChoices([], null, 'gone').at(-1), { kind: 'missing', value: 'gone' })
  assert.equal(composerStyleChoices([], null, 'default').length, 1)
})
