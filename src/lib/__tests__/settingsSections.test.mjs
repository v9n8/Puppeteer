// Unit tests for lib/settingsSections.ts (Settings nav groups + feature gating).
// Run: node --test src/lib/__tests__/settingsSections.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { SETTINGS_GROUPS, SETTINGS_SECTIONS, groupLabelKey, settingsNav } from '../settingsSections.ts'

const prop = (id, effective) => ({
  id, stored: null, effective, locked: false, default: effective, degraded: [], reason_key: null,
})
// Every gated section's feature, switched on or off together.
const treeAll = (on) => [
  { module: 'productivity', enabled: on, locked: false, props: [prop('productivity.focus', on)] },
  { module: 'app', enabled: true, locked: false, props: [prop('app.command_palette', on)] },
  { module: 'infra_access', enabled: on, locked: false, props: [prop('infra_access.aws', on), prop('infra_access.azure', on)] },
  { module: 'integrations', enabled: on, locked: false, props: [prop('integrations.google', on)] },
]
const shape = (nav) => nav.map((g) => [g.id, g.sections.map((s) => s.id)])

test('every section belongs to a known group, ids are unique', () => {
  const groups = new Set(SETTINGS_GROUPS.map((g) => g.id))
  for (const s of SETTINGS_SECTIONS) assert.ok(groups.has(s.group), `${s.id} → ${s.group}`)
  assert.equal(new Set(SETTINGS_SECTIONS.map((s) => s.id)).size, SETTINGS_SECTIONS.length)
})

test('settingsNav: all features on → four groups in order, every section listed', () => {
  assert.deepEqual(shape(settingsNav(SETTINGS_SECTIONS, treeAll(true))), [
    ['general', ['general', 'features', 'focus', 'palette', 'keymap']],
    ['ai', ['engines', 'ai', 'styles', 'translate', 'sessions']],
    ['connections', ['connections', 'inbox']],
    ['system', ['devices']],
  ])
})

test('settingsNav: features off hide their sections, ungated ones stay', () => {
  assert.deepEqual(shape(settingsNav(SETTINGS_SECTIONS, treeAll(false))), [
    ['general', ['general', 'features']],
    ['ai', ['engines', 'ai', 'styles', 'translate', 'sessions']],
    ['connections', ['connections', 'inbox']],
    ['system', ['devices']],
  ])
})

test('settingsNav: a group whose sections are all off gets no heading', () => {
  const sections = [
    { id: 'a', group: 'general' },
    { id: 'b', group: 'connections', feature: 'infra_access.aws' },
    { id: 'c', group: 'system' },
  ]
  const off = [{ module: 'infra_access', enabled: false, locked: false, props: [prop('infra_access.aws', false)] }]
  assert.deepEqual(shape(settingsNav(sections, off)), [['general', ['a']], ['system', ['c']]])
  const on = [{ module: 'infra_access', enabled: true, locked: false, props: [prop('infra_access.aws', true)] }]
  assert.deepEqual(shape(settingsNav(sections, on)), [['general', ['a']], ['connections', ['b']], ['system', ['c']]])
})

test('settingsNav: groups follow SETTINGS_GROUPS order, not the input order', () => {
  const sections = [{ id: 'd', group: 'system' }, { id: 'x', group: 'ai' }, { id: 'g', group: 'general' }]
  assert.deepEqual(settingsNav(sections, []).map((g) => g.id), ['general', 'ai', 'system'])
})

test('group and section labels exist in en and vi', () => {
  for (const locale of ['en', 'vi']) {
    const msgs = JSON.parse(readFileSync(new URL(`../../i18n/locales/${locale}/settings.json`, import.meta.url), 'utf8'))
    const get = (key) => key.split('.').slice(1).reduce((o, k) => o?.[k], msgs)
    for (const g of SETTINGS_GROUPS) assert.equal(typeof get(groupLabelKey(g.id)), 'string', `${locale} ${g.labelKey}`)
    for (const s of SETTINGS_SECTIONS) assert.equal(typeof get(s.labelKey), 'string', `${locale} ${s.labelKey}`)
  }
})
