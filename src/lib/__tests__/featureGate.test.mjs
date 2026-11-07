// Unit tests for lib/featureGate.ts (feature checks on the tree `get_features`
// returns). Run: node --test src/lib/__tests__/featureGate.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { filterByFeature, isFeatureOn } from '../featureGate.ts'

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
