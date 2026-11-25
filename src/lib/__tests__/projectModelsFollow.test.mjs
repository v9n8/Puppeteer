// Project › Model and the global default model: a default the new list moves onto
// the global one follows it, and the save warns when the project takes its default
// model from Settings while the new list turns that model off.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { projectValueForPick } from '../defaultMark.ts'
import { FOLLOWED_MODEL_KEYS, blockedFollowers } from '../globalDefaultWarning.ts'
import { ALL_MODELS_ON, firstEnabled, intersectEnabled } from '../enabledModels.ts'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')
const settings = { default_engine: 'claude', claude_model: 'claude-opus-5-5' }

test('a pinned default moved onto the global default model follows it; onto another model it stays pinned', () => {
  const onto = intersectEnabled(ALL_MODELS_ON, { claude: ['claude-opus-5-5', 'claude-haiku-4-5'], codex: null })
  assert.equal(projectValueForPick(firstEnabled('claude', onto), settings.claude_model), null)
  const other = intersectEnabled(ALL_MODELS_ON, { claude: ['claude-haiku-4-5'], codex: null })
  assert.equal(projectValueForPick(firstEnabled('claude', other), settings.claude_model), 'claude-haiku-4-5')
})

test('a list that turns off the default model a following project starts on is caught; a pinned one is not', () => {
  const lists = { claude: ['claude-sonnet-4-6'], codex: null }
  const on = () => intersectEnabled(ALL_MODELS_ON, lists)
  const following = [{ id: 'p', name: 'P', session_defaults: { enabled_models: lists } }]
  assert.deepEqual(blockedFollowers(following, settings, on, FOLLOWED_MODEL_KEYS), [{ id: 'p', name: 'P' }])
  const pinned = [{ id: 'p', name: 'P', session_defaults: { model_claude: 'claude-sonnet-4-6', enabled_models: lists } }]
  assert.deepEqual(blockedFollowers(pinned, settings, on, FOLLOWED_MODEL_KEYS), [])
  const keeps = () => intersectEnabled(ALL_MODELS_ON, { claude: ['claude-opus-5-5'], codex: null })
  assert.deepEqual(blockedFollowers(following, settings, keeps, FOLLOWED_MODEL_KEYS), [])
})

test('Project › Model saves moves through the follow rule and warns in the confirm dialog', () => {
  const src = read('../../components/projects/ProjectModels.vue')
  assert.match(src, /projectValueForPick\(m\.to, globalModels\[m\.engine\]\) \|\| null/)
  assert.doesNotMatch(src, /= m\.to \|\| null/)
  assert.match(src, /lines\.push\(t\('projectDetail\.model\.confirmFollowedOff', \{ model: followedOff \}\)\)/)
  // The dialog shows for a move or for the warning, before anything is written.
  assert.ok(src.indexOf('if (lines.length)') < src.indexOf('await projectStore.setSessionDefaults'))
  for (const lang of ['en', 'vi']) {
    const model = JSON.parse(read(`../../i18n/locales/${lang}/projectDetail.json`)).model
    assert.match(model.confirmFollowedOff, /\{model\}/, lang)
  }
})
