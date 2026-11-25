// Project defaults follow the global default: picking the ★ option in a
// project-default select stores "follow" (null), any other option pins.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { projectDefaultSelect, projectValueForPick } from '../defaultMark.ts'
import { composerDefaults } from '../composerChips.ts'
import { NO_STYLE } from '../responseStyles.ts'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')

test('picking the starred option follows the global value; another option pins', () => {
  assert.equal(projectValueForPick('claude-opus-5-5', 'claude-opus-5-5'), null)
  assert.equal(projectValueForPick('claude-sonnet-4-6', 'claude-opus-5-5'), 'claude-sonnet-4-6')
  // A pinned copy of an old global: after the global moved, the old value is no longer ★.
  assert.equal(projectValueForPick('claude-opus-5-5', 'claude-opus-5-5[1m]'), 'claude-opus-5-5')
  assert.equal(projectValueForPick('claude-opus-5-5[1m]', 'claude-opus-5-5[1m]'), null)
})

test('a following project shows the global value starred, and moves with it', () => {
  const models = [{ value: 'claude-opus-5-5', label: '200k' }, { value: 'claude-opus-5-5[1m]', label: '1M' }]
  const before = projectDefaultSelect(models, null, 'claude-opus-5-5')
  assert.equal(before.value, 'claude-opus-5-5')
  const after = projectDefaultSelect(models, null, 'claude-opus-5-5[1m]')
  assert.equal(after.value, 'claude-opus-5-5[1m]')
  assert.deepEqual(after.options.filter((o) => o.isDefault).map((o) => o.value), ['claude-opus-5-5[1m]'])
  // A pinned other model stays and is not starred.
  assert.equal(projectDefaultSelect(models, 'claude-sonnet-4-6', 'claude-opus-5-5[1m]').value, 'claude-sonnet-4-6')
})

test('the starred value of each field is the resolved global, so its pick follows', () => {
  // Blank / invalid global engine is shown as Claude; picking Claude follows.
  const blank = composerDefaults({ settings: { default_engine: '' } })
  assert.equal(projectValueForPick('claude', blank.engine), null)
  assert.equal(projectValueForPick('codex', blank.engine), 'codex')
  // Blank global permission mode resolves to "default".
  assert.equal(projectValueForPick('default', composerDefaults({ settings: {} }).permissionMode), null)
  // An invalid global effort stars nothing but the "engine decides" row.
  const effort = composerDefaults({ settings: { default_effort: 'bogus' } }).effort
  assert.equal(effort, '')
  assert.equal(projectValueForPick('high', effort), 'high')
})

test('"Default (engine decides)" while the global model is blank follows', () => {
  const opts = [{ value: '', label: 'Default' }, { value: 'opus', label: 'opus' }]
  const select = projectDefaultSelect(opts, null, '')
  assert.equal(select.options[0].isDefault, true)
  assert.equal(projectValueForPick('', ''), null)
})

test('"None" style while the global style is blank follows', () => {
  // The project page stars `global.style || NO_STYLE`.
  const starred = composerDefaults({ settings: { default_style: '' } }).style || NO_STYLE
  assert.equal(starred, NO_STYLE)
  assert.equal(projectValueForPick(NO_STYLE, starred), null)
  assert.equal(projectValueForPick('terse', starred), 'terse')
})

test('the project page saves what a pick stores and passes each select\'s starred value', () => {
  const src = read('../../components/ProjectSessionDefaults.vue')
  assert.match(src, /const next = projectValueForPick\(picked, starred\)/)
  // The old "unchanged" check swallowed a re-pick of a pinned ★ value.
  assert.doesNotMatch(src, /\(own\.value\[field\] \?\? ''\) === value/)
  assert.match(src, /\(own\.value\[field\]\?\.trim\(\) \|\| null\) === next\) return/)
  for (const [field, starred] of [
    ['engine', 'global.engine'],
    ['model_claude', 'global.models.claude'],
    ['model_codex', 'global.models.codex'],
    ['permission_mode', 'global.permissionMode'],
    ['effort', 'global.effort'],
    ['style', 'globalStyle'],
  ]) {
    assert.ok(src.includes(`save('${field}', $event, ${starred})`), field)
  }
  // Only the picked field changes; the others are sent as they are.
  assert.match(src, /setSessionDefaults\(props\.project\.id, \{ \.\.\.own\.value, \[field\]: next \}\)/)
})
