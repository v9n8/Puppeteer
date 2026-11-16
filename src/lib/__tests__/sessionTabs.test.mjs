// Unit tests for lib/sessionTabs.ts (the Session area's file tabs, V3-24).
// Run: node --test src/lib/__tests__/sessionTabs.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  CONVERSATION, MAX_SESSIONS, closeOthers, closeTab, emptyState, fileTabIcon, hiddenTabIndexes, openTab, orderedTabs,
  parse, prune, renameTab, selectTab, serialize, tabLabel, togglePin,
} from '../sessionTabs.ts'

const ids = (s) => orderedTabs(s).map((t) => t.id)

function three() {
  let s = emptyState()
  s = openTab(s, 'a.ts', null)
  s = openTab(s, 'b.ts', null)
  return openTab(s, 'c.ts', 7)
}

test('emptyState: only Conversation, selected', () => {
  const s = emptyState(5)
  assert.deepEqual(s.tabs, [])
  assert.equal(s.active, CONVERSATION)
  assert.equal(s.usedAt, 5)
})

test('openTab: adds at the end and selects it; input is not mutated', () => {
  const before = emptyState()
  const s = openTab(before, 'src/a.ts', 12, 100)
  assert.deepEqual(before.tabs, [])
  assert.deepEqual(s.tabs, [{ id: 'file:src/a.ts', kind: 'file', path: 'src/a.ts', line: 12, pinned: false }])
  assert.equal(s.active, 'file:src/a.ts')
  assert.equal(s.usedAt, 100)
  assert.deepEqual(ids(openTab(s, 'src/b.ts', null)), ['file:src/a.ts', 'file:src/b.ts'])
})

test('openTab: an open file is only selected, its line updated, never duplicated', () => {
  let s = three()
  s = selectTab(s, CONVERSATION)
  s = openTab(s, 'a.ts', 42)
  assert.equal(s.tabs.length, 3)
  assert.equal(s.active, 'file:a.ts')
  assert.equal(s.tabs[0].line, 42)
  // No line given: keep the last one.
  assert.equal(openTab(s, 'a.ts', null).tabs[0].line, 42)
})

test('selectTab: unknown ids are ignored', () => {
  const s = three()
  assert.equal(selectTab(s, 'file:nope').active, 'file:c.ts')
  assert.equal(selectTab(s, CONVERSATION).active, CONVERSATION)
})

test('closeTab: the selected tab hands over to its left neighbour, Conversation last', () => {
  let s = three()
  s = closeTab(s, 'file:c.ts')
  assert.deepEqual(ids(s), ['file:a.ts', 'file:b.ts'])
  assert.equal(s.active, 'file:b.ts')
  s = selectTab(s, 'file:a.ts')
  s = closeTab(s, 'file:a.ts')
  assert.equal(s.active, CONVERSATION)
})

test('closeTab: closing another tab keeps the selection; Conversation cannot close', () => {
  const s = three()
  assert.equal(closeTab(s, 'file:a.ts').active, 'file:c.ts')
  assert.equal(closeTab(s, CONVERSATION), s)
})

test('closeTab: the left neighbour is taken in shown order (pinned first)', () => {
  let s = togglePin(three(), 'file:c.ts') // c, a, b
  s = selectTab(s, 'file:a.ts')
  s = closeTab(s, 'file:a.ts')
  assert.equal(s.active, 'file:c.ts')
})

test('closeOthers: keeps Conversation, pinned tabs and the one asked for', () => {
  let s = togglePin(three(), 'file:a.ts')
  s = closeOthers(s, 'file:c.ts')
  assert.deepEqual(ids(s), ['file:a.ts', 'file:c.ts'])
  assert.equal(s.active, 'file:c.ts')
})

test('closeOthers: a selected tab that goes hands over to the kept one', () => {
  let s = selectTab(three(), 'file:b.ts')
  s = closeOthers(s, 'file:a.ts')
  assert.deepEqual(ids(s), ['file:a.ts'])
  assert.equal(s.active, 'file:a.ts')
})

test('togglePin: pinned tabs move right after Conversation; unpin keeps them unpinned', () => {
  let s = togglePin(three(), 'file:c.ts')
  assert.deepEqual(ids(s), ['file:c.ts', 'file:a.ts', 'file:b.ts'])
  assert.equal(s.tabs.find((t) => t.id === 'file:c.ts').pinned, true)
  s = togglePin(s, 'file:b.ts')
  assert.deepEqual(ids(s), ['file:c.ts', 'file:b.ts', 'file:a.ts'])
  s = togglePin(s, 'file:c.ts')
  assert.equal(s.tabs.find((t) => t.id === 'file:c.ts').pinned, false)
  assert.deepEqual(ids(s), ['file:b.ts', 'file:c.ts', 'file:a.ts'])
  assert.equal(togglePin(s, CONVERSATION), s)
})

test('renameTab: follows a file and files under a renamed folder, selection included', () => {
  let s = emptyState()
  s = openTab(s, 'src/x/a.ts', 3)
  s = openTab(s, 'src/x/b.ts', null)
  s = openTab(s, 'src/xy.ts', null)
  s = selectTab(s, 'file:src/x/a.ts')
  s = renameTab(s, 'src/x', 'lib/x')
  assert.deepEqual(ids(s), ['file:lib/x/a.ts', 'file:lib/x/b.ts', 'file:src/xy.ts'])
  assert.equal(s.active, 'file:lib/x/a.ts')
  assert.equal(s.tabs[0].line, 3)
})

test('renameTab: onto a path that already has a tab keeps that tab only', () => {
  let s = emptyState()
  s = openTab(s, 'a.ts', null)
  s = openTab(s, 'b.ts', null)
  s = selectTab(s, 'file:a.ts')
  s = renameTab(s, 'a.ts', 'b.ts')
  assert.deepEqual(ids(s), ['file:b.ts'])
  assert.equal(s.active, 'file:b.ts')
})

test('prune: drops gone and empty Sessions, keeps the most recent MAX_SESSIONS', () => {
  const all = {}
  for (let i = 0; i < MAX_SESSIONS + 5; i++) all[`r${i}`] = openTab(emptyState(), 'a.ts', null, i)
  all.empty = emptyState(9999)
  const kept = prune(all, null, 10_000)
  assert.equal(Object.keys(kept).length, MAX_SESSIONS)
  assert.equal(kept.empty, undefined)
  assert.equal(kept.r0, undefined)
  assert.ok(kept[`r${MAX_SESSIONS + 4}`])
  assert.deepEqual(Object.keys(prune(all, new Set(['r1', 'gone']), 10_000)), ['r1'])
})

test('prune: a usedAt in the future is clamped to now', () => {
  const kept = prune({ r: openTab(emptyState(), 'a.ts', null, 5000) }, null, 1000)
  assert.equal(kept.r.usedAt, 1000)
})

test('serialize → parse round-trips', () => {
  let s = togglePin(three(), 'file:b.ts')
  s = { ...s, split: { on: false, ratio: 0.5, rightActive: 'file:a.ts' } }
  const all = { r1: s, r2: openTab(emptyState(), '/abs/log.jsonl', null, 3) }
  assert.deepEqual(parse(serialize(all)), all)
})

test('parse: broken JSON or wrong shapes are empty; bad records and tabs are dropped', () => {
  assert.deepEqual(parse(null), {})
  assert.deepEqual(parse('{not json'), {})
  assert.deepEqual(parse('[1,2]'), {})
  const raw = JSON.stringify({
    ok: {
      tabs: [{ kind: 'file', path: 'a.ts', line: 3, pinned: true }, { kind: 'file', path: '' }, { kind: 'file', path: 'a.ts' },
        { kind: 'file', path: 'b.ts', line: -1 }, { kind: 'changed' }, 'junk'],
      active: 'file:gone.ts',
      usedAt: 'later',
    },
    bad: { tabs: 'nope' },
    worse: 7,
  })
  const out = parse(raw)
  assert.deepEqual(Object.keys(out), ['ok'])
  assert.deepEqual(out.ok.tabs, [
    { id: 'file:a.ts', kind: 'file', path: 'a.ts', line: 3, pinned: true },
    { id: 'file:b.ts', kind: 'file', path: 'b.ts', line: null, pinned: false },
    { id: 'changed', kind: 'changed', pinned: false },
  ])
  // The selected tab is gone: back to Conversation.
  assert.equal(out.ok.active, CONVERSATION)
  assert.equal(out.ok.usedAt, 0)
})

test('tabLabel: name and folder', () => {
  assert.deepEqual(tabLabel('src/auth/refresh.ts'), { name: 'refresh.ts', dir: 'src/auth' })
  assert.deepEqual(tabLabel('README.md'), { name: 'README.md', dir: '' })
})

test('fileTabIcon: by extension', () => {
  assert.equal(fileTabIcon('src/a.vue'), 'code')
  assert.equal(fileTabIcon('package.json'), 'json')
  assert.equal(fileTabIcon('shot.PNG'), 'image')
  assert.equal(fileTabIcon('docs/auth.md'), 'text')
  assert.equal(fileTabIcon('Makefile'), 'file')
  assert.equal(fileTabIcon('.env'), 'file')
})

test('hiddenTabIndexes: tabs cut by either edge of the viewport', () => {
  const boxes = [{ left: 0, width: 100 }, { left: 100, width: 100 }, { left: 200, width: 100 }, { left: 300, width: 100 }]
  assert.deepEqual(hiddenTabIndexes(boxes, 0, 400), [])
  assert.deepEqual(hiddenTabIndexes(boxes, 0, 250), [2, 3])
  assert.deepEqual(hiddenTabIndexes(boxes, 150, 250), [0, 1])
  // Sub-pixel slack: the third tab still counts as shown.
  assert.deepEqual(hiddenTabIndexes(boxes, 0, 299.5), [3])
})
