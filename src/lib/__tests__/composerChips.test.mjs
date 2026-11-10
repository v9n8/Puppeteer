// Unit tests for lib/composerChips.ts (UX-26): the defaults and choices of the
// composer's permission mode / effort / response style chips.
// Run: node --test src/lib/__tests__/composerChips.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  composerDefaults, effortForLaunch, effectiveEffort, inheritedStyle, composerStyleChoices,
} from '../composerChips.ts'
import { effortLevelsFor } from '../engineOptions.ts'

test('defaults: global settings, with unset or invalid values normalised', () => {
  assert.deepEqual(composerDefaults({ settings: null }), { permissionMode: 'default', effort: '', style: '' })
  assert.deepEqual(
    composerDefaults({ settings: { default_permission_mode: 'plan', default_effort: ' high ', default_style: 'concise' } }),
    { permissionMode: 'plan', effort: 'high', style: 'concise' },
  )
  assert.deepEqual(
    composerDefaults({ settings: { default_permission_mode: 'nonsense', default_effort: 'turbo', default_style: '  ' } }),
    { permissionMode: 'default', effort: '', style: '' },
  )
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
  assert.equal(inheritedStyle('explain_junior', 'my-1', known), 'explain_junior')
  // Before the list loads, every reference counts.
  assert.equal(inheritedStyle('gone', 'my-1', null), 'gone')
})

const styles = [
  { id: 'g1', project_id: null, name: 'Mine', description: 'global' },
  { id: 'p1', project_id: 'proj-a', name: 'Project A', description: '' },
  { id: 'p2', project_id: 'proj-b', name: 'Project B', description: '' },
]

test('style choices: none, built-ins, my global styles, then this project\'s', () => {
  const choices = composerStyleChoices(styles, 'proj-a', 'concise')
  assert.deepEqual(choices.map((c) => [c.kind, c.value]), [
    ['none', 'default'],
    ['builtin', 'concise'],
    ['builtin', 'explain_junior'],
    ['builtin', 'code_review'],
    ['mine', 'g1'],
    ['project', 'p1'],
  ])
  assert.deepEqual(choices.at(-1), { kind: 'project', value: 'p1', name: 'Project A', description: '' })
})

test('style choices: another project\'s or a deleted style stays listed as missing', () => {
  assert.deepEqual(composerStyleChoices(styles, 'proj-a', 'p2').at(-1), { kind: 'missing', value: 'p2' })
  assert.deepEqual(composerStyleChoices([], null, 'gone').at(-1), { kind: 'missing', value: 'gone' })
  assert.equal(composerStyleChoices([], null, 'default').length, 4)
})
