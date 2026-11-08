// Unit tests for lib/featureGate.ts (feature checks on the tree `get_features`
// returns). Run: node --test src/lib/__tests__/featureGate.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  CONFIRM_ON,
  ENTRY_FEATURES,
  FALLBACK_PATH,
  MENU_ACTION_FEATURES,
  allowsFeature,
  confirmOnKey,
  entryOn,
  errorText,
  fallbackPath,
  featureErrorMessage,
  featureLabelKey,
  featureRows,
  filterByFeature,
  isFeatureOn,
  menuActionFeature,
  pickAvailable,
  popoutFeature,
  routeFeature,
  startsGroup,
} from '../featureGate.ts'

const prop = (id, effective, extra = {}) => ({
  id,
  stored: null,
  effective,
  locked: false,
  default: effective,
  degraded: [],
  reason_key: null,
  ...extra,
})

// Shaped like the backend's resolve(): props of a module that is off are
// already off, locked ones stay on, a degraded prop stays on.
const tree = [
  {
    module: 'productivity',
    enabled: true,
    locked: false,
    props: [
      prop('productivity.notes', true),
      prop('productivity.todos', true, { stored: true }),
      prop('productivity.focus', true, {
        stored: true,
        degraded: ['productivity.calendar'],
        reason_key: 'features.reason.missing_dependency',
      }),
      prop('productivity.calendar', false),
    ],
  },
  {
    module: 'code_hosting',
    enabled: false,
    locked: false,
    props: [
      prop('code_hosting.issue_pr_runs', true, { locked: true, reason_key: 'features.reason.locked' }),
      prop('code_hosting.pr_inbox', false, { stored: true, reason_key: 'features.reason.module_off' }),
    ],
  },
  {
    module: 'text_tools',
    enabled: true,
    locked: false,
    props: [prop('text_tools.selection.translate', true), prop('text_tools.selection.save_note', false)],
  },
]

test('isFeatureOn reads modules and props', () => {
  assert.equal(isFeatureOn(tree, 'productivity'), true)
  assert.equal(isFeatureOn(tree, 'productivity.notes'), true)
  assert.equal(isFeatureOn(tree, 'productivity.calendar'), false)
})

test('a module that is off turns its props off, except locked ones', () => {
  assert.equal(isFeatureOn(tree, 'code_hosting'), false)
  // Stored on, but its module is off.
  assert.equal(isFeatureOn(tree, 'code_hosting.pr_inbox'), false)
  assert.equal(isFeatureOn(tree, 'code_hosting.issue_pr_runs'), true)
})

test('a prop with a missing dependency stays on', () => {
  assert.equal(isFeatureOn(tree, 'productivity.focus'), true)
  assert.equal(isFeatureOn(tree, 'productivity.calendar'), false)
})

test('dotted prop ids match whole, never split', () => {
  assert.equal(isFeatureOn(tree, 'text_tools.selection.translate'), true)
  assert.equal(isFeatureOn(tree, 'text_tools.selection.save_note'), false)
  assert.equal(isFeatureOn(tree, 'text_tools.selection'), false)
  assert.equal(isFeatureOn(tree, 'selection.translate'), false)
})

test('unknown ids and an empty (not yet loaded) tree are off', () => {
  for (const id of ['', 'nope', 'productivity.nope', 'notes', 'feature.productivity']) {
    assert.equal(isFeatureOn(tree, id), false, id)
  }
  assert.equal(isFeatureOn([], 'productivity'), false)
  assert.equal(isFeatureOn([], 'productivity.notes'), false)
})

test('filterByFeature keeps ungated items and those whose feature is on', () => {
  const routes = [
    { path: '/projects' },
    { path: '/focus', feature: 'productivity.focus' },
    { path: '/pr-inbox', feature: 'code_hosting.pr_inbox' },
    { path: '/calendar', feature: 'productivity.calendar' },
    { path: '/notes', feature: 'productivity.notes' },
    { path: '/gone', feature: 'nope' },
  ]
  assert.deepEqual(
    filterByFeature(routes, tree).map((r) => r.path),
    ['/projects', '/focus', '/notes'],
  )
  // Turning the module off hides every route it gates.
  const off = tree.map((m) =>
    m.module === 'productivity'
      ? { ...m, enabled: false, props: m.props.map((p) => ({ ...p, effective: false })) }
      : m,
  )
  assert.deepEqual(filterByFeature(routes, off).map((r) => r.path), ['/projects'])
  // Not loaded yet: only ungated items.
  assert.deepEqual(filterByFeature(routes, []).map((r) => r.path), ['/projects'])
})

// Shaped like lib/navigation's NAV_ROUTES (which imports Vue, so not loadable here).
const navRoutes = [
  { path: '/projects', group: 'work' },
  { path: '/focus', group: 'work', feature: 'productivity.focus' },
  { path: '/pr-inbox', group: 'work', feature: 'code_hosting.pr_inbox' },
  { path: '/calendar', group: 'work', feature: 'productivity.calendar' },
  { path: '/skills', group: 'config' },
  { path: '/servers', group: 'config', feature: 'infra_access.vps' },
  { path: '/stats', group: 'app', feature: 'usage.stats' },
  { path: '/settings', group: 'app' },
]

test('allowsFeature: ungated always, gated only when on', () => {
  assert.equal(allowsFeature(tree, undefined), true)
  assert.equal(allowsFeature([], undefined), true)
  assert.equal(allowsFeature(tree, 'productivity.notes'), true)
  assert.equal(allowsFeature(tree, 'code_hosting.pr_inbox'), false)
})

test('group dividers follow the filtered list', () => {
  const groups = (items) => items.map((r, i) => (startsGroup(items, i) ? `| ${r.path}` : r.path))
  assert.deepEqual(groups(navRoutes), [
    '/projects', '/focus', '/pr-inbox', '/calendar', '| /skills', '/servers', '| /stats', '/settings',
  ])
  // Stats off: the app group still opens with a divider, on Settings.
  assert.deepEqual(groups(filterByFeature(navRoutes, tree)), ['/projects', '/focus', '| /skills', '| /settings'])
  assert.equal(startsGroup([], 0), false)
})

test('routeFeature matches the route and the paths under it, never a prefix of a name', () => {
  assert.equal(routeFeature('/focus', navRoutes), 'productivity.focus')
  assert.equal(routeFeature('/servers/abc', navRoutes), 'infra_access.vps')
  assert.equal(routeFeature('/stats', navRoutes), 'usage.stats')
  assert.equal(routeFeature('/statsx', navRoutes), undefined)
  assert.equal(routeFeature('/projects/p1/run/r1', navRoutes), undefined)
  assert.equal(routeFeature('/settings', navRoutes), undefined)
})

test('menuActionFeature: Go items follow their route, others the table', () => {
  assert.equal(menuActionFeature('go./pr-inbox', navRoutes), 'code_hosting.pr_inbox')
  assert.equal(menuActionFeature('go./projects', navRoutes), undefined)
  assert.equal(menuActionFeature('file.newTodo', navRoutes), 'productivity.todos')
  assert.equal(menuActionFeature('file.newNote', navRoutes), 'productivity.notes')
  assert.equal(menuActionFeature('window.secretNotes', navRoutes), 'security.secret_vault')
  assert.equal(menuActionFeature('window.slackThreads', navRoutes), 'integrations.slack')
  assert.equal(menuActionFeature('file.newSession', navRoutes), undefined)
  assert.equal(menuActionFeature('view.toggleSidebar', navRoutes), undefined)
  // With the tree above: todos on, Slack off (unknown to it).
  assert.equal(allowsFeature(tree, menuActionFeature('window.todoList', navRoutes)), true)
  assert.equal(allowsFeature(tree, menuActionFeature('window.slackThreads', navRoutes)), false)
  assert.equal(allowsFeature(tree, menuActionFeature('help.reload', navRoutes)), true)
})

test('popoutFeature reads the window flag; the list window goes by its kind', () => {
  assert.equal(popoutFeature('?focusWindow=1'), 'productivity.focus')
  assert.equal(popoutFeature('?prInboxWindow=1'), 'code_hosting.pr_inbox')
  assert.equal(popoutFeature('?calendarWindow=1'), 'productivity.calendar')
  assert.equal(popoutFeature('?ganttWindow=1&project=p1'), 'code_hosting.gantt')
  assert.equal(popoutFeature('?secretNotesWindow=1&id=x'), 'security.secret_vault')
  assert.equal(popoutFeature('?slackWindow=1'), 'integrations.slack')
  assert.equal(popoutFeature('?itemListWindow=1&kind=note'), 'productivity.notes')
  assert.equal(popoutFeature('?itemListWindow=1&kind=todo'), 'productivity.todos')
  assert.equal(popoutFeature('?itemListWindow=1'), 'productivity.todos')
  // Ungated windows and the main window.
  for (const search of ['', '?fileWindow=1', '?permissionWindow=1', '?itemWindow=1', '?focusWindow=0']) {
    assert.equal(popoutFeature(search), undefined, search)
  }
})

const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8')

/** Module and full prop ids of the backend registry (src-tauri/src/features/mod.rs). */
function registryIds() {
  const modules = new Set()
  const props = new Set()
  const blocks = read('../../../src-tauri/src/features/mod.rs')
    .split('pub static MODULES')[1]
    .split('\n];')[0]
    .split(/Module \{/)
  for (const block of blocks.slice(1)) {
    const module = block.match(/id: "(\w+)"/)[1]
    modules.add(module)
    for (const [, prop] of block.matchAll(/(?:core|on|off)\("([\w.]+)"\)/g)) props.add(`${module}.${prop}`)
  }
  return { modules, props }
}

// Every id the UI gates on must exist in the backend registry: an unknown id
// reads as off, so a typo would hide its entry for good.
test('gated ids exist in the backend registry', () => {
  const { modules, props } = registryIds()
  const registry = new Set([...modules, ...props])
  assert.ok(registry.has('text_tools.selection.translate'), 'registry parsed')

  const used = new Set([
    ...Object.values(MENU_ACTION_FEATURES),
    ...['focusWindow', 'prInboxWindow', 'calendarWindow', 'ganttWindow', 'secretNotesWindow', 'slackWindow']
      .map((flag) => popoutFeature(`?${flag}=1`)),
    ...['todo', 'note'].map((kind) => popoutFeature(`?itemListWindow=1&kind=${kind}`)),
    ...Object.values(ENTRY_FEATURES).flat(),
  ])
  for (const [file, pattern] of [
    ['../navigation.ts', /feature: '([\w.]+)'/g],
    ['../tray.ts', /isOn\('([\w.]+)'\)/g],
    ['../../App.vue', /isOn\('([\w.]+)'\)/g],
  ]) {
    for (const [, id] of read(file).matchAll(pattern)) used.add(id)
  }
  for (const file of ['focus', 'prInbox', 'calendar', 'gantt', 'secretNotes', 'slack', 'itemList']) {
    for (const [, id] of read(`../${file}Window.ts`).matchAll(/'((?:productivity|code_hosting|security|integrations)\.\w+)'/g)) {
      used.add(id)
    }
  }
  assert.ok(used.size >= 10, `found ${used.size} ids`)
  for (const id of used) assert.ok(registry.has(id), `${id} is not in the registry`)
})

test('featureRows: a prop under a module that is off shows its own value, dimmed', () => {
  const [productivity, codeHosting, textTools] = featureRows(tree, false)

  assert.deepEqual(
    { checked: productivity.checked, editable: productivity.editable, locked: productivity.locked, onCount: productivity.onCount },
    { checked: true, editable: true, locked: false, onCount: 3 },
  )
  assert.deepEqual(productivity.props[2], {
    id: 'productivity.focus',
    checked: true,
    editable: true,
    muted: false,
    reasonKey: 'features.reason.missing_dependency',
    degraded: ['productivity.calendar'],
  })
  // Never saved: its default.
  assert.equal(productivity.props[3].checked, false)

  assert.equal(codeHosting.checked, false)
  assert.equal(codeHosting.onCount, 1, 'only the locked prop is in effect')
  const [runs, prInbox] = codeHosting.props
  // Stored on: back on once the module is; meanwhile dimmed and not editable.
  assert.deepEqual(
    { checked: prInbox.checked, editable: prInbox.editable, muted: prInbox.muted, reasonKey: prInbox.reasonKey },
    { checked: true, editable: false, muted: true, reasonKey: 'features.reason.module_off' },
  )
  assert.deepEqual(
    { checked: runs.checked, editable: runs.editable, muted: runs.muted, reasonKey: runs.reasonKey },
    { checked: true, editable: false, muted: true, reasonKey: 'features.reason.locked' },
  )

  assert.deepEqual(textTools.props.map((p) => [p.id, p.checked]), [
    ['text_tools.selection.translate', true],
    ['text_tools.selection.save_note', false],
  ])
})

test('featureRows: locked modules and read-only windows edit nothing', () => {
  const locked = [{ module: 'sessions', enabled: true, locked: true, props: [prop('sessions.handoff', true)] }]
  const [sessions] = featureRows(locked, false)
  assert.equal(sessions.editable, false)
  assert.equal(sessions.props[0].editable, true, 'a prop of a locked module is still a switch')

  for (const row of featureRows(tree, true)) {
    assert.equal(row.editable, false, row.module)
    for (const p of row.props) assert.equal(p.editable, false, p.id)
  }
  assert.deepEqual(featureRows([], false), [])
})

// Settings → Features names every module and prop; a missing label would show
// the raw i18n key.
test('every registry id has a label in en and vi', () => {
  const { modules, props } = registryIds()
  const at = (tree, path) => path.split('.').reduce((node, key) => node?.[key], tree)
  for (const locale of ['en', 'vi']) {
    const messages = JSON.parse(read(`../../i18n/locales/${locale}/features.json`))
    for (const id of modules) assert.equal(typeof messages.module[id], 'string', `${locale}: module ${id}`)
    for (const id of props) assert.equal(typeof at(messages.prop, id), 'string', `${locale}: prop ${id}`)
    for (const key of ['locked', 'module_off', 'missing_dependency']) {
      assert.equal(typeof messages.reason[key], 'string', `${locale}: reason ${key}`)
    }
  }
})

// ── Entry points outside the sidebar / menus (D0-19) ────────────────────────

/** A tree where exactly the ids in `on` are on (modules on, so props decide). */
function treeWith(on) {
  const ids = new Set(Object.values(ENTRY_FEATURES).flat())
  const byModule = new Map()
  for (const id of ids) {
    const module = id.slice(0, id.indexOf('.'))
    if (!byModule.has(module)) byModule.set(module, [])
    byModule.get(module).push(prop(id, on.includes(id)))
  }
  return [...byModule].map(([module, props]) => ({ module, enabled: true, locked: false, props }))
}

test('entryOn: every entry hides once one of its features is off', () => {
  const all = [...new Set(Object.values(ENTRY_FEATURES).flat())]
  for (const [entry, needs] of Object.entries(ENTRY_FEATURES)) {
    assert.equal(entryOn(treeWith(all), entry), true, `${entry} with everything on`)
    for (const id of needs) {
      assert.equal(entryOn(treeWith(all.filter((x) => x !== id)), entry), false, `${entry} without ${id}`)
    }
  }
  assert.equal(entryOn([], 'run.handoff'), false, 'nothing loaded: hidden')
})

test('entryOn: the Calendar entries need both Calendar and the Google account', () => {
  assert.deepEqual([...ENTRY_FEATURES['focus.calendar']].sort(), ['integrations.google', 'productivity.calendar'])
  assert.equal(entryOn(treeWith(['productivity.calendar']), 'focus.calendar'), false)
  assert.equal(entryOn(treeWith(['integrations.google']), 'focus.calendar'), false)
  assert.equal(entryOn(treeWith(['productivity.calendar', 'integrations.google']), 'focus.calendar'), true)
})

test('entryOn: a module that is off hides its entries', () => {
  const off = [{ module: 'productivity', enabled: false, locked: false, props: [prop('productivity.todos', false, { stored: true })] }]
  assert.equal(entryOn(off, 'item.todo'), false)
  assert.equal(entryOn(off, 'todo.add'), false)
})

test('pickAvailable keeps the current choice, else the first offered', () => {
  assert.equal(pickAvailable('note', ['todo', 'note']), 'note')
  assert.equal(pickAvailable('todo', ['note']), 'note', 'its tab is gone: the other one')
  assert.equal(pickAvailable('todo', []), null, 'nothing offered')
  assert.equal(pickAvailable('slack', ['all', 'note']), 'all', 'a filter falls back to All')
})

test('fallbackPath leaves the page whose feature was switched off', () => {
  const routes = [{ path: '/projects' }, { path: '/servers', feature: 'infra_access.vps' }, { path: '/stats', feature: 'usage.stats' }]
  const vpsOff = [{ module: 'infra_access', enabled: true, locked: false, props: [prop('infra_access.vps', false)] }]
  assert.equal(fallbackPath('/servers', routes, vpsOff), FALLBACK_PATH)
  assert.equal(fallbackPath('/servers/abc', routes, vpsOff), FALLBACK_PATH, 'a page under it too')
  assert.equal(fallbackPath('/projects/p1/runs/r1', routes, vpsOff), null, 'ungated: stays')
  const vpsOn = [{ module: 'infra_access', enabled: true, locked: false, props: [prop('infra_access.vps', true)] }]
  assert.equal(fallbackPath('/servers', routes, vpsOn), null)
  assert.equal(FALLBACK_PATH, '/projects')
})

// A fake `t`: the key with its params, so a test sees which message was picked.
const t = (key, params) => (params ? `${key}${JSON.stringify(params)}` : key)

test('featureLabelKey: module vs prop (whose id may hold more dots)', () => {
  assert.equal(featureLabelKey('productivity'), 'features.module.productivity')
  assert.equal(featureLabelKey('productivity.focus'), 'features.prop.productivity.focus')
  assert.equal(featureLabelKey('text_tools.selection.translate'), 'features.prop.text_tools.selection.translate')
})

test('featureErrorMessage maps the backend refusals to i18n', () => {
  assert.equal(
    featureErrorMessage('feature_disabled: remote.remote_control', t),
    'features.error.disabled{"name":"features.prop.remote.remote_control"}',
  )
  assert.equal(
    featureErrorMessage(new Error('feature_disabled: integrations'), t),
    'features.error.disabled{"name":"features.module.integrations"}',
    'a module, inside an Error',
  )
  assert.equal(
    featureErrorMessage('feature_disabled: infra_access.aws, infra_access.azure', t),
    'features.error.disabled{"name":"features.prop.infra_access.aws, features.prop.infra_access.azure"}',
  )
  assert.equal(featureErrorMessage('remote_denied_local_only', t), 'features.error.localOnly')
  assert.equal(featureErrorMessage('Error: remote_denied_local_only: set_feature', t), 'features.error.localOnly')
  assert.equal(featureErrorMessage('relay unreachable', t), null)
})

test('errorText keeps any other error as it came', () => {
  assert.equal(errorText('relay unreachable', t), 'relay unreachable')
  assert.equal(errorText(new Error('boom'), t), 'Error: boom')
  assert.equal(errorText('feature_disabled: usage.stats', t), 'features.error.disabled{"name":"features.prop.usage.stats"}')
})

test('the feature error messages exist in en and vi', () => {
  for (const locale of ['en', 'vi']) {
    const messages = JSON.parse(read(`../../i18n/locales/${locale}/features.json`))
    assert.match(messages.error.disabled, /\{name\}/, `${locale}: disabled names the feature`)
    assert.equal(typeof messages.error.localOnly, 'string', `${locale}: localOnly`)
  }
})

test('confirmOnKey: only switching a listed id on asks first', () => {
  assert.equal(confirmOnKey('library.skills_global', true), 'features.confirmOn.skills_global')
  assert.equal(confirmOnKey('library.skills_global', false), null)
  assert.equal(confirmOnKey('library.skills', true), null)
  assert.equal(confirmOnKey('library', true), null)
})

test('confirm-on ids exist in the registry and their dialog text in en and vi', () => {
  const { props } = registryIds()
  const at = (tree, path) => path.split('.').reduce((node, key) => node?.[key], tree)
  for (const [id, key] of Object.entries(CONFIRM_ON)) {
    assert.ok(props.has(id), id)
    for (const locale of ['en', 'vi']) {
      const messages = { features: JSON.parse(read(`../../i18n/locales/${locale}/features.json`)) }
      for (const part of ['title', 'message', 'confirm']) {
        assert.equal(typeof at(messages, `${key}.${part}`), 'string', `${locale}: ${key}.${part}`)
      }
    }
  }
})
