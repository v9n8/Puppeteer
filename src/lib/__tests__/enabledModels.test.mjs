// Unit tests for lib/enabledModels.ts (models that are on, V3-62).
// Run: node --test src/lib/__tests__/enabledModels.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { parseEnabledModels, modelBlockedError, ALL_MODELS_ON } from '../enabledModels.ts'

test('unset, broken or null means every model is on', () => {
  for (const raw of [undefined, null, '', 'not json', '[]', '{}', '{"claude":null,"codex":null}', 42, ['x']]) {
    assert.deepEqual(parseEnabledModels(raw), ALL_MODELS_ON, String(raw))
  }
})

test('lists are trimmed, deduplicated and keep their order', () => {
  assert.deepEqual(
    parseEnabledModels('{"claude":[" claude-sonnet-4-6 ","",3,"claude-sonnet-4-6","claude-haiku-4-5"],"codex":"x"}'),
    { claude: ['claude-sonnet-4-6', 'claude-haiku-4-5'], codex: null },
  )
  // An empty list is kept: only "Default (engine picks)" is left.
  assert.deepEqual(parseEnabledModels('{"codex":[]}'), { claude: null, codex: [] })
})

test('a phone frame carries the object itself', () => {
  assert.deepEqual(parseEnabledModels({ claude: ['claude-opus-4-8'], codex: null }), {
    claude: ['claude-opus-4-8'],
    codex: null,
  })
})

test('the backend refusal codes are recognized with their model', () => {
  assert.deepEqual(
    modelBlockedError('model_disabled:claude-opus-4-8: Model claude-opus-4-8 is turned off in Settings › Model.'),
    { code: 'modelDisabled', model: 'claude-opus-4-8' },
  )
  assert.deepEqual(
    modelBlockedError(new Error('model_default_disabled:claude-opus-4-8[1m]: Model … is the default model')),
    { code: 'modelDefaultDisabled', model: 'claude-opus-4-8[1m]' },
  )
  for (const other of ['Run is already active', 'budget: over', 'model_disabled', 'x model_disabled:a: b']) {
    assert.equal(modelBlockedError(other), null, other)
  }
})
