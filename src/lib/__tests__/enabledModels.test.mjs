// Unit tests for lib/enabledModels.ts (models that are on, V3-62).
// Run: node --test src/lib/__tests__/enabledModels.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  parseEnabledModels, modelBlockedError, ALL_MODELS_ON,
  isModelOff, modelBlocks, draftOn, engineListToSave, draftSummary,
} from '../enabledModels.ts'
import { filterEnabledOptions, MODEL_OPTIONS } from '../engineOptions.ts'

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

// Settings › Model (V3-63) ------------------------------------------------------

const ON = { claude: ['claude-sonnet-4-6', 'claude-haiku-4-5'], codex: ['gpt-5.5'] }

test('isModelOff: default, aliases and null lists are never off', () => {
  assert.equal(isModelOff('claude', '', ON), false)
  assert.equal(isModelOff('claude', 'sonnet', ON), false)
  assert.equal(isModelOff('claude', 'claude-opus-4-8', null), false)
  assert.equal(isModelOff('claude', 'claude-opus-4-8', { claude: null, codex: [] }), false)
  assert.equal(isModelOff('claude', 'claude-opus-4-8', ON), true)
  assert.equal(isModelOff('codex', 'gpt-5.4', ON), true)
  // A fallback list is off when any of its ids is.
  assert.equal(isModelOff('claude', 'claude-sonnet-4-6, claude-haiku-4-5', ON), false)
  assert.equal(isModelOff('claude', 'claude-sonnet-4-6, claude-opus-4-8', ON), true)
})

test('filterEnabledOptions: null keeps everything', () => {
  assert.deepEqual(filterEnabledOptions(MODEL_OPTIONS.claude, null, 'claude', ''), MODEL_OPTIONS.claude)
  assert.deepEqual(filterEnabledOptions(MODEL_OPTIONS.codex, ALL_MODELS_ON, 'codex', ''), MODEL_OPTIONS.codex)
})

test('filterEnabledOptions: keeps Default, the alias and the models that are on', () => {
  const opts = [{ value: '', label: 'Off' }, { value: 'sonnet', label: 'sonnet' }, ...MODEL_OPTIONS.claude.slice(1)]
  assert.deepEqual(
    filterEnabledOptions(opts, ON, 'claude', '').map((o) => o.value),
    ['', 'sonnet', 'claude-sonnet-4-6', 'claude-haiku-4-5'],
  )
})

test('filterEnabledOptions: an empty list leaves only Default', () => {
  assert.deepEqual(
    filterEnabledOptions(MODEL_OPTIONS.codex, { claude: null, codex: [] }, 'codex', '').map((o) => o.value),
    [''],
  )
})

test('filterEnabledOptions: the stored id stays, marked turned off', () => {
  const out = filterEnabledOptions(MODEL_OPTIONS.claude, ON, 'claude', 'claude-opus-4-8')
  assert.deepEqual(out.map((o) => o.value), ['', 'claude-opus-4-8', 'claude-sonnet-4-6', 'claude-haiku-4-5'])
  const kept = out.find((o) => o.value === 'claude-opus-4-8')
  assert.equal(kept.label, 'claude-opus-4-8 · turned off')
  assert.equal(kept.description, 'Opus 4.8 · 200K context')
  // A stored id of the other engine is not added here.
  assert.equal(filterEnabledOptions(MODEL_OPTIONS.claude, ON, 'claude', 'gpt-5.4').length, 3)
})

const IDS = ['a', 'b', 'c', 'd']

test('modelBlocks: on and usable are available; off or unusable are not', () => {
  const { available, unavailable } = modelBlocks(IDS, ['a', 'b', 'c'], { b: { ok: false, error: 'no 1M' }, c: { ok: true } })
  assert.deepEqual(available.map((r) => r.id), ['a', 'c'])
  assert.deepEqual(unavailable, [
    { id: 'b', on: true, usable: false, reason: 'no 1M' },
    { id: 'd', on: false, usable: true },
  ])
  // The tab count ("Claude 4/4"): null = all on.
  assert.equal(modelBlocks(IDS, null, {}).available.length, 4)
})

test('draftOn flips the saved state of a changed row', () => {
  assert.equal(draftOn(['a'], new Set(), 'a'), true)
  assert.equal(draftOn(['a'], new Set(['a']), 'a'), false)
  assert.equal(draftOn(['a'], new Set(['b']), 'b'), true)
  assert.equal(draftOn(null, new Set(['b']), 'b'), false)
})

test('engineListToSave: nothing changed keeps the saved value, else display order', () => {
  assert.equal(engineListToSave(IDS, null, new Set()), null)
  assert.deepEqual(engineListToSave(IDS, null, new Set(['b'])), ['a', 'c', 'd'])
  assert.deepEqual(engineListToSave(IDS, ['d', 'a'], new Set(['b'])), ['a', 'b', 'd'])
  // A saved id no longer listed is kept at the end.
  assert.deepEqual(engineListToSave(IDS, ['gone', 'a'], new Set(['a'])), ['gone'])
  assert.deepEqual(engineListToSave(IDS, ['a'], new Set(['a'])), [])
})

test('draftSummary lists what turns on and off', () => {
  assert.deepEqual(draftSummary(IDS, ['a', 'b'], new Set(['b', 'c'])), { on: ['c'], off: ['b'] })
})
