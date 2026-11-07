// Unit tests for lib/featureGate.ts (feature checks on the tree `get_features`
// returns). Run: node --test src/lib/__tests__/featureGate.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  MENU_ACTION_FEATURES,
  allowsFeature,
  featureRows,
  filterByFeature,
  isFeatureOn,
  menuActionFeature,
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
