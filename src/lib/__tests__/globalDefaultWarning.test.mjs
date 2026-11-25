// Saving a global engine or default model warns about the projects that follow
// it but whose own model list (Project › Model) turns it off.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { blockedFollowers, namesShown } from '../globalDefaultWarning.ts'
import { ALL_MODELS_ON, intersectEnabled } from '../enabledModels.ts'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')

const NEW_MODEL = 'claude-opus-5-5[1m]'
// Lists of each project's own models (Settings has every model on).
const lists = {
  c: { claude: ['claude-opus-5-5', 'claude-sonnet-4-6'], codex: null },
  d: { claude: ['claude-sonnet-4-6'], codex: null },
  e: { claude: [NEW_MODEL], codex: ['gpt-5.4'] },
}
const enabledFor = (id) => intersectEnabled(ALL_MODELS_ON, lists[id] ?? null)
const settings = { default_engine: 'claude', claude_model: NEW_MODEL, codex_model: 'gpt-5.5' }

test('a project following the model whose own list turns the new model off is named', () => {
  const projects = [{ id: 'c', name: 'C', session_defaults: {} }]
  assert.deepEqual(blockedFollowers(projects, settings, enabledFor), [{ id: 'c', name: 'C' }])
})

test('a project pinned to a model its list allows is not named', () => {
  const projects = [{ id: 'd', name: 'D', session_defaults: { model_claude: 'claude-sonnet-4-6' } }]
  assert.deepEqual(blockedFollowers(projects, settings, enabledFor), [])
})

test('nothing blocked gives no names', () => {
  const projects = [
    { id: 'e', name: 'E', session_defaults: null },
    { id: 'free', name: 'Free', session_defaults: {} },
  ]
  assert.deepEqual(blockedFollowers(projects, settings, enabledFor), [])
})

test('a new global engine makes a project that follows it use a Codex model its list turns off', () => {
  const projects = [{ id: 'e', name: 'E', session_defaults: {} }]
  assert.deepEqual(blockedFollowers(projects, settings, enabledFor), [])
  assert.deepEqual(blockedFollowers(projects, { ...settings, default_engine: 'codex' }, enabledFor), [{ id: 'e', name: 'E' }])
  // Its own engine pinned to Claude: the global engine change does not reach it.
  const pinned = [{ id: 'e', name: 'E', session_defaults: { engine: 'claude' } }]
  assert.deepEqual(blockedFollowers(pinned, { ...settings, default_engine: 'codex' }, enabledFor), [])
})

test('the check only reads: projects and their lists are left as they were', () => {
  const projects = [{ id: 'c', name: 'C', session_defaults: { effort: 'low' } }]
  const before = JSON.stringify({ projects, lists })
  blockedFollowers(projects, settings, enabledFor)
  assert.equal(JSON.stringify({ projects, lists }), before)
})

test('up to three names are shown, then how many more', () => {
  assert.deepEqual(namesShown(['A', 'B']), { names: 'A, B', more: 0 })
  assert.deepEqual(namesShown(['A', 'B', 'C', 'D', 'E']), { names: 'A, B, C', more: 2 })
})

test('the settings auto-save warns after "Saved", from the store\'s live projects only', () => {
  const form = read('../../components/settings/useSettingsForm.ts')
  const saved = form.indexOf("toast.success(t('settings.toast.saved'))")
  assert.ok(saved > 0 && form.indexOf('warnBlockedFollowers(changed.map') > saved)
  const warn = read('../../components/settings/warnBlockedFollowers.ts')
  // `list_projects` leaves the Trash out, so a trashed project is never named.
  assert.match(warn, /if \(!projects\.projects\.length\) await projects\.fetchProjects\(\)/)
  assert.match(warn, /toast\.info\(t\('settings\.ai\.followersBlocked', \{ names \}\), 8000\)/)
  for (const lang of ['en', 'vi']) {
    const ai = JSON.parse(read(`../../i18n/locales/${lang}/settings.json`)).ai
    assert.match(ai.followersBlocked, /\{names\}/, lang)
    assert.match(ai.followersMore, /\{names\}.*\{count\}/, lang)
  }
})
