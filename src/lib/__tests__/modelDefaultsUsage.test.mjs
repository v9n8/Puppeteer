// Unit tests for lib/modelDefaultsUsage.ts (Settings › Model badges, V3-63).
// Run: node --test src/lib/__tests__/modelDefaultsUsage.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { modelDefaultsUsage, defaultChangePlace } from '../modelDefaultsUsage.ts'

test('each default names its model under its engine', () => {
  const usage = modelDefaultsUsage(
    {
      claude_model: 'claude-opus-5-5',
      codex_model: 'gpt-5.5',
      claude_fallback_model: 'sonnet, claude-haiku-4-5',
      translate_engine: '',
      translate_model: 'claude-haiku-4-5',
    },
    [
      { id: 'p1', name: 'A', session_defaults: { model_claude: 'claude-haiku-4-5', model_codex: null } },
      { id: 'p2', name: 'B', session_defaults: null },
    ],
  )
  assert.deepEqual(usage.claude['claude-opus-5-5'], [{ place: 'sessionDefault' }])
  assert.deepEqual(usage.codex['gpt-5.5'], [{ place: 'sessionDefault' }])
  assert.deepEqual(usage.claude['claude-haiku-4-5'], [
    { place: 'fallback' },
    { place: 'translate' },
    { place: 'project', projectId: 'p1', projectName: 'A' },
  ])
  assert.deepEqual(usage.claude.sonnet, [{ place: 'fallback' }])
})

test('blank values name no model; translate follows its engine', () => {
  const usage = modelDefaultsUsage(
    { claude_model: '', codex_model: ' ', translate_engine: 'codex', translate_model: 'gpt-5.4' },
    [],
  )
  assert.deepEqual(usage.claude, {})
  assert.deepEqual(usage.codex, { 'gpt-5.4': [{ place: 'translate' }] })
  assert.deepEqual(modelDefaultsUsage(null, []), { claude: {}, codex: {} })
})

test('defaultChangePlace maps the backend place to the screen that edits it', () => {
  const c = (place) => defaultChangePlace({ place, from: 'x', to: 'y' })
  assert.deepEqual(c('claude_model'), { place: 'sessionDefault', section: 'ai' })
  assert.deepEqual(c('codex_model'), { place: 'sessionDefault', section: 'ai' })
  assert.deepEqual(c('claude_fallback_model'), { place: 'fallback', section: 'ai' })
  assert.deepEqual(c('translate_model'), { place: 'translate', section: 'translate' })
  assert.deepEqual(c('focus.ai_model'), { place: 'focus' })
  assert.deepEqual(c('project.default_model_claude'), { place: 'project' })
})
