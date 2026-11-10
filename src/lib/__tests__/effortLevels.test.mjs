// Unit tests for the effort helpers of lib/engineOptions.ts (UX-16): which
// reasoning effort levels a run on an engine + model can use.
// Run: node --test src/lib/__tests__/effortLevels.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { EFFORT_LEVELS, claudeEffortLevels, effortLevelsFor } from '../engineOptions.ts'

const ALL = [...EFFORT_LEVELS]

test('claude: current models, aliases and the engine default take every level', () => {
  for (const m of ['claude-opus-5-5', 'claude-opus-5-5[1m]', 'claude-fable-5-1', 'opus', 'haiku', '', null]) {
    assert.deepEqual(claudeEffortLevels(m), ALL, String(m))
  }
})

test('claude: models without effort get none, whatever the spelling', () => {
  for (const m of ['claude-haiku-4-5', 'claude-haiku-4-5-20251001', 'claude-sonnet-4-5[1m]',
    'us.anthropic.claude-sonnet-4-5-20250929-v1:0', 'claude-sonnet-4-5@20250929', 'claude-3-7-sonnet-20250219']) {
    assert.deepEqual(claudeEffortLevels(m), [], m)
  }
})

test('claude: older effort models lose xhigh / max', () => {
  assert.deepEqual(claudeEffortLevels('claude-sonnet-4-6[1m]'), ['low', 'medium', 'high', 'max'])
  assert.deepEqual(claudeEffortLevels('claude-opus-4-6'), ['low', 'medium', 'high', 'max'])
  assert.deepEqual(claudeEffortLevels('claude-opus-4-5'), ['low', 'medium', 'high'])
})

test('codex: no max; the discovered catalog narrows the levels per model', () => {
  assert.deepEqual(effortLevelsFor('codex', ''), ['low', 'medium', 'high', 'xhigh'])
  const codex = [
    { value: 'gpt-5.5', label: 'gpt-5.5', effort_levels: ['low', 'medium', 'high', 'xhigh'] },
    { value: 'gpt-mini', label: 'gpt-mini', effort_levels: ['minimal', 'low', 'medium'] },
    { value: 'gpt-old', label: 'gpt-old' },
  ]
  assert.deepEqual(effortLevelsFor('codex', 'gpt-mini', { codex }), ['low', 'medium'])
  assert.deepEqual(effortLevelsFor('codex', 'gpt-old', { codex }), ['low', 'medium', 'high', 'xhigh'])
  assert.deepEqual(effortLevelsFor('codex', 'unknown', { codex }), ['low', 'medium', 'high', 'xhigh'])
})

test('effortLevelsFor: claude (and unknown engines) follow the Claude table', () => {
  assert.deepEqual(effortLevelsFor('claude', 'claude-haiku-4-5'), [])
  assert.deepEqual(effortLevelsFor('claude', 'claude-opus-5-5'), ALL)
})
