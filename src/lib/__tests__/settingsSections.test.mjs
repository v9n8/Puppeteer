// Unit tests for lib/settingsSections.ts (Settings nav groups + feature gating).
// Run: node --test src/lib/__tests__/settingsSections.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  SETTINGS_GROUPS, SETTINGS_SECTIONS, groupLabelKey, isSettingsSectionId, resolveSettingsPlace, settingsCrumbKeys,
  settingsHref, settingsLocation, mcpLegacyLocation, settingsNav, settingsPlaceFromRoute, settingsRedirect,
} from '../settingsSections.ts'

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
    ['general', ['general', 'flags', 'features', 'focus', 'shortcuts']],
    ['ai', ['engines', 'model', 'ai', 'styles', 'translate', 'memory', 'limits']],
    ['connections', ['connections', 'mcp', 'inbox']],
    ['system', ['devices', 'about']],
  ])
})

test('settingsNav: features off hide their sections, ungated ones stay', () => {
  assert.deepEqual(shape(settingsNav(SETTINGS_SECTIONS, treeAll(false))), [
    ['general', ['general', 'flags', 'features']],
    ['ai', ['engines', 'model', 'ai', 'styles', 'translate', 'memory', 'limits']],
    ['connections', ['connections', 'mcp', 'inbox']],
    ['system', ['devices', 'about']],
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

test('resolveSettingsPlace: current ids, merged old ids, tabs and connection filters', () => {
  assert.deepEqual(resolveSettingsPlace('memory'), { section: 'memory' })
  assert.deepEqual(resolveSettingsPlace('model'), { section: 'model' })
  assert.deepEqual(resolveSettingsPlace('claude'), { section: 'engines' })
  assert.deepEqual(resolveSettingsPlace('usage'), { section: 'limits' })
  assert.deepEqual(resolveSettingsPlace('sessions'), { section: 'limits' })
  assert.deepEqual(resolveSettingsPlace('limits'), { section: 'limits' })
  assert.deepEqual(resolveSettingsPlace('remote'), { section: 'devices', devicesTab: 'remote' })
  assert.deepEqual(resolveSettingsPlace('sync'), { section: 'devices', devicesTab: 'backup' })
  assert.deepEqual(resolveSettingsPlace('palette'), { section: 'shortcuts', shortcutsTab: 'palette' })
  assert.deepEqual(resolveSettingsPlace('keymap'), { section: 'shortcuts', shortcutsTab: 'keymap' })
  assert.deepEqual(resolveSettingsPlace('shortcuts'), { section: 'shortcuts' })
  assert.deepEqual(resolveSettingsPlace('google'), { section: 'connections', connectionsFilter: 'google' })
})

test('resolveSettingsPlace: no section or an unknown one keeps what is open', () => {
  assert.equal(resolveSettingsPlace(undefined), null)
  assert.equal(resolveSettingsPlace(['ai']), null)
  assert.equal(resolveSettingsPlace('nope'), null)
})

test('settingsRedirect: every old id of resolveSettingsPlace lands on its route', () => {
  const cases = {
    claude: ['/settings/engines', {}],
    engine: ['/settings/engines', {}],
    usage: ['/settings/limits', {}],
    sessions: ['/settings/limits', {}],
    remote: ['/settings/devices', { tab: 'remote' }],
    sync: ['/settings/devices', { tab: 'backup' }],
    backup: ['/settings/devices', { tab: 'backup' }],
    palette: ['/settings/shortcuts', { tab: 'palette' }],
    keymap: ['/settings/shortcuts', { tab: 'keymap' }],
    github: ['/settings/connections', { type: 'github' }],
    gitlab: ['/settings/connections', { type: 'gitlab' }],
    aws: ['/settings/connections', { type: 'aws' }],
    azure: ['/settings/connections', { type: 'azure' }],
    google: ['/settings/connections', { type: 'google' }],
  }
  for (const [id, [path, query]] of Object.entries(cases)) {
    assert.deepEqual(settingsRedirect({ section: id }), { path, query }, id)
  }
})

test('settingsRedirect: current ids, no id and unknown ids', () => {
  for (const s of SETTINGS_SECTIONS) {
    assert.deepEqual(settingsRedirect({ section: s.id }), { path: `/settings/${s.id}`, query: {} }, s.id)
  }
  assert.deepEqual(settingsRedirect({}), { path: '/settings/general', query: {} })
  assert.deepEqual(settingsRedirect({ section: 'nope' }), { path: '/settings/general', query: {} })
  assert.deepEqual(settingsRedirect({ section: ['ai'] }), { path: '/settings/general', query: {} })
})

test("settingsRedirect: the link's other query keys are kept", () => {
  assert.deepEqual(settingsRedirect({ section: 'github', new: '1', junk: ['a'] }), {
    path: '/settings/connections',
    query: { new: '1', type: 'github' },
  })
})

test('settingsPlaceFromRoute: section, tab and filter from the route', () => {
  assert.deepEqual(settingsPlaceFromRoute({ section: 'memory' }, {}), { section: 'memory' })
  assert.deepEqual(settingsPlaceFromRoute({ section: 'devices' }, { tab: 'backup' }), { section: 'devices', devicesTab: 'backup' })
  assert.deepEqual(
    settingsPlaceFromRoute({ section: 'connections' }, { type: 'google' }),
    { section: 'connections', connectionsFilter: 'google' },
  )
  assert.deepEqual(settingsPlaceFromRoute({ section: 'shortcuts' }, { tab: 'keymap' }), { section: 'shortcuts', shortcutsTab: 'keymap' })
  assert.deepEqual(settingsPlaceFromRoute({ section: 'keymap' }, {}), { section: 'shortcuts', shortcutsTab: 'keymap' })
  assert.deepEqual(settingsPlaceFromRoute({ section: 'shortcuts' }, { tab: 'nope' }), { section: 'shortcuts' })
  // Old ids in the path resolve like a deep link; unknown ones show General.
  assert.deepEqual(settingsPlaceFromRoute({ section: 'github' }, {}), { section: 'connections', connectionsFilter: 'github' })
  assert.deepEqual(settingsPlaceFromRoute({ section: 'nope' }, {}), { section: 'general' })
  assert.deepEqual(settingsPlaceFromRoute({}, {}), { section: 'general' })
  // A tab / filter that isn't known or doesn't belong to the section is ignored.
  assert.deepEqual(settingsPlaceFromRoute({ section: 'devices' }, { tab: 'nope', type: 'github' }), { section: 'devices' })
  assert.deepEqual(settingsPlaceFromRoute({ section: 'general' }, { tab: 'remote' }), { section: 'general' })
})

test('settingsLocation / settingsHref round-trip through settingsPlaceFromRoute', () => {
  const places = [
    { section: 'general' },
    { section: 'devices', devicesTab: 'remote' },
    { section: 'shortcuts', shortcutsTab: 'keymap' },
    { section: 'connections', connectionsFilter: 'azure' },
  ]
  for (const p of places) {
    const loc = settingsLocation(p)
    assert.deepEqual(settingsPlaceFromRoute({ section: loc.path.split('/')[2] }, loc.query), p)
  }
  assert.equal(settingsHref(settingsLocation({ section: 'memory' })), '/settings/memory')
  assert.equal(settingsHref(settingsLocation({ section: 'devices', devicesTab: 'backup' })), '/settings/devices?tab=backup')
})

test('isSettingsSectionId and settingsCrumbKeys', () => {
  assert.equal(isSettingsSectionId('memory'), true)
  assert.equal(isSettingsSectionId('github'), false)
  assert.equal(isSettingsSectionId(undefined), false)
  assert.deepEqual(settingsCrumbKeys('general'), ['settings.title', 'settings.groups.general', 'settings.sections.general'])
  assert.deepEqual(
    settingsCrumbKeys('connections'),
    ['settings.title', 'settings.groups.connections', 'settings.sections.connections'],
  )
})

test('mcp: a section of the connections group, between Connections and Inbox API', () => {
  const ids = SETTINGS_SECTIONS.filter((s) => s.group === 'connections').map((s) => s.id)
  assert.deepEqual(ids, ['connections', 'mcp', 'inbox'])
  assert.deepEqual(resolveSettingsPlace('mcp'), { section: 'mcp' })
  assert.deepEqual(settingsRedirect({ section: 'mcp' }), { path: '/settings/mcp', query: {} })
  assert.deepEqual(settingsPlaceFromRoute({ section: 'mcp' }, { id: 'x' }), { section: 'mcp' })
})

test('mcpLegacyLocation: /mcp, /mcp/new and /mcp/<id>/edit land on Settings › MCP', () => {
  assert.deepEqual(mcpLegacyLocation({}), { path: '/settings/mcp', query: {} })
  assert.deepEqual(mcpLegacyLocation({ id: 'abc' }), { path: '/settings/mcp', query: { id: 'abc' } })
  assert.deepEqual(mcpLegacyLocation({ id: 'builtin:google' }), { path: '/settings/mcp', query: { id: 'builtin:google' } })
  assert.deepEqual(mcpLegacyLocation({ new: '1' }), { path: '/settings/mcp', query: { new: '1' } })
  assert.deepEqual(mcpLegacyLocation({ new: '0', other: 'z' }), { path: '/settings/mcp', query: {} })
  assert.deepEqual(mcpLegacyLocation({}, 'abc'), { path: '/settings/mcp', query: { id: 'abc' } })
})
