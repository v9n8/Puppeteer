// Unit tests for lib/menuGroups.ts (menu actions grouped by top-level menu).
// Run: node --test src/lib/__tests__/menuGroups.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { groupMenuActions } from '../menuGroups.ts'

const item = (id, label = id) => ({ kind: 'item', id, label })
const SEP = { kind: 'separator' }
const spec = [
  { label: 'File', items: [item('file.new'), SEP, item('go./settings')] },
  { label: 'Edit', items: [{ kind: 'predefined', role: 'copy' }] },
  {
    label: 'Help',
    items: [item('help.docs'), SEP, { kind: 'submenu', items: [item('help.reload')] }, item('file.new')],
  },
]

test('groups follow menu order; a menu with no action is dropped', () => {
  assert.deepEqual(groupMenuActions(spec).map((g) => g.menu), ['File', 'Help'])
})

test('a nested submenu joins its parent group, separators and roles are skipped', () => {
  const help = groupMenuActions(spec).find((g) => g.menu === 'Help')
  assert.deepEqual(help.actions.map((a) => a.id), ['help.docs', 'help.reload'])
})

test('an id is in one group only (the first)', () => {
  const ids = groupMenuActions(spec).flatMap((g) => g.actions.map((a) => a.id))
  assert.equal(new Set(ids).size, ids.length)
  assert.deepEqual(groupMenuActions(spec)[0].actions.map((a) => a.id), ['file.new', 'go./settings'])
})

test('extra actions join their menu, or a new group at the end', () => {
  const win = { id: 'window.open', label: 'Open windows' }
  const added = groupMenuActions(spec, [{ menu: 'Help', action: win }])
  assert.deepEqual(added.find((g) => g.menu === 'Help').actions.at(-1), win)
  const own = groupMenuActions(spec, [{ menu: 'Window', action: win }])
  assert.deepEqual(own.at(-1), { menu: 'Window', actions: [win] })
})

test('no menus, no groups; without the macOS app menu File comes first', () => {
  assert.deepEqual(groupMenuActions([]), [])
  assert.equal(groupMenuActions(spec)[0].menu, 'File')
})
