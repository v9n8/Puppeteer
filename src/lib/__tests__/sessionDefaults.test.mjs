// Unit tests for lib/sessionDefaults.ts (UX-18): the effort and response style
// choices of Settings › "Defaults for new sessions".
// Run: node --test src/lib/__tests__/sessionDefaults.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { defaultEffortChoices, defaultStyleChoices } from '../sessionDefaults.ts'
import { effortLevelsFor } from '../engineOptions.ts'

const values = (choices) => choices.map((c) => c.value)

test('effort: engine default first, then the levels the default engine + model take', () => {
  assert.deepEqual(values(defaultEffortChoices(effortLevelsFor('claude', ''), '')), ['', 'low', 'medium', 'high', 'xhigh', 'max'])
  assert.deepEqual(values(defaultEffortChoices(effortLevelsFor('codex', ''), 'high')), ['', 'low', 'medium', 'high', 'xhigh'])
  assert.ok(defaultEffortChoices(effortLevelsFor('codex', ''), 'high').every((c) => c.supported))
})

test('effort: a saved level the model does not take stays listed, marked unsupported', () => {
  const choices = defaultEffortChoices(effortLevelsFor('codex', 'gpt-5.5'), 'max')
  assert.deepEqual(values(choices), ['', 'low', 'medium', 'high', 'xhigh', 'max'])
  assert.deepEqual(choices.at(-1), { value: 'max', supported: false })
  // A model without effort: only "engine decides" (+ the saved level); the screen hides the select.
  assert.deepEqual(effortLevelsFor('claude', 'claude-haiku-4-5'), [])
  assert.deepEqual(values(defaultEffortChoices([], '')), [''])
})

const mine = [
  { id: 'concise', project_id: null, name: 'Concise', description: 'sample' },
  { id: 's1', project_id: null, name: 'Short', description: 'one line' },
  { id: 's2', project_id: 'p1', name: 'Project only', description: '' },
]

test('style (UX-36): None, then the global styles, the samples included', () => {
  const choices = defaultStyleChoices(mine, '')
  assert.deepEqual(choices.map((c) => [c.kind, c.value]), [
    ['none', ''],
    ['mine', 'concise'],
    ['mine', 's1'],
  ])
  assert.equal(choices.at(-1).name, 'Short')
})

test('style: a saved "default" is None, listed once and kept as it is', () => {
  assert.deepEqual(defaultStyleChoices([], 'default'), [{ kind: 'none', value: 'default' }])
})

test('style: a saved value that resolves to nothing stays listed as missing', () => {
  assert.deepEqual(defaultStyleChoices(mine, 'gone').at(-1), { kind: 'missing', value: 'gone' })
  // Another project's style does not resolve globally either, nor a deleted sample.
  assert.deepEqual(defaultStyleChoices(mine, 's2').at(-1), { kind: 'missing', value: 's2' })
  assert.deepEqual(defaultStyleChoices([], 'code_review').at(-1), { kind: 'missing', value: 'code_review' })
  assert.equal(defaultStyleChoices(mine, 'concise').length, 3)
})
