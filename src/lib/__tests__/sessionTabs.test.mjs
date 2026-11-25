// Unit tests for lib/sessionTabs.ts (the Session area's file tabs, V3-24).
// Run: node --test src/lib/__tests__/sessionTabs.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  CONVERSATION, MAX_SESSIONS, clampRatio, closeOthers, closeTab, effectiveLayout, emptyState, fileAreaTab, fileTabIcon, hiddenTabIndexes,
  openChangedTab, openTab, orderedTabs, ratioLimits, resetRatio, setRatio, splitBlockedReason, startSplit, stopSplit,
  moveTab, parse, prune, removeTabsUnder, renameTab, selectTab, serialize, tabLabel, togglePin,
  dedupeTabs, fileTabId, normalizeTabPath,
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

test('openChangedTab: opens the tab once, then only selects it; it closes and pins like a file tab', () => {
  let s = openChangedTab(openTab(emptyState(), 'a.ts', null))
  assert.deepEqual(ids(s), ['file:a.ts', 'changed'])
  assert.equal(s.active, 'changed')
  s = openChangedTab(selectTab(s, 'file:a.ts'))
  assert.equal(s.tabs.length, 2)
  assert.equal(s.active, 'changed')
  assert.deepEqual(ids(togglePin(s, 'changed')), ['changed', 'file:a.ts'])
  assert.equal(closeTab(s, 'changed').active, 'file:a.ts')
  assert.deepEqual(parse(serialize({ r: s })).r.tabs.map((t) => t.id), ['file:a.ts', 'changed'])
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

// ── Split view (V3-27) ─────────────────────────────────────────────────────

test('startSplit: needs a tab; right side is the given tab, else the selected file, else the first', () => {
  const none = emptyState()
  assert.equal(startSplit(none), none)
  let s = three()
  s = selectTab(s, CONVERSATION)
  assert.equal(startSplit(s).split.rightActive, 'file:a.ts')
  assert.equal(startSplit(s, 'file:b.ts').split.rightActive, 'file:b.ts')
  assert.equal(startSplit(selectTab(s, 'file:c.ts')).split.rightActive, 'file:c.ts')
  assert.equal(startSplit(s).split.on, true)
  assert.equal(startSplit(s).split.ratio, 0.5)
  // Splitting again keeps the ratio and only moves the right side.
  const again = startSplit(setRatio(startSplit(s), 900, 1200), 'file:c.ts')
  assert.equal(again.split.rightActive, 'file:c.ts')
  assert.equal(again.split.ratio, 0.7)
})

test('stopSplit: back to tabs, the right tab stays selected', () => {
  let s = startSplit(three(), 'file:b.ts')
  s = selectTab(s, CONVERSATION)
  const off = stopSplit(s)
  assert.equal(off.split.on, false)
  assert.equal(off.active, 'file:b.ts')
  assert.equal(stopSplit(emptyState()).split, undefined)
})

test('split: opening or selecting a file goes to the right; Conversation stays left', () => {
  let s = startSplit(three(), 'file:a.ts')
  s = openTab(s, 'd.ts', 3)
  assert.equal(s.split.rightActive, 'file:d.ts')
  s = selectTab(s, CONVERSATION)
  assert.equal(s.split.rightActive, 'file:d.ts')
  assert.equal(s.active, CONVERSATION)
  s = openChangedTab(s)
  assert.equal(s.split.rightActive, 'changed')
  // Not split: no split state appears.
  assert.equal(openTab(three(), 'x.ts', null).split, undefined)
})

test('split: closing tabs moves the right side left; the last one ends the split', () => {
  let s = startSplit(three(), 'file:b.ts')
  s = closeTab(s, 'file:b.ts')
  assert.equal(s.split.rightActive, 'file:a.ts')
  assert.equal(s.active, 'file:a.ts')
  s = closeTab(s, 'file:a.ts')
  assert.equal(s.split.rightActive, 'file:c.ts')
  s = closeTab(s, 'file:c.ts')
  assert.equal(s.split.on, false)
  assert.equal(s.active, CONVERSATION)
  // Closing a tab that is not on the right leaves it.
  const t = closeTab(startSplit(three(), 'file:b.ts'), 'file:c.ts')
  assert.equal(t.split.rightActive, 'file:b.ts')
  const o = closeOthers(startSplit(three(), 'file:b.ts'), 'file:c.ts')
  assert.equal(o.split.rightActive, 'file:c.ts')
})

test('clampRatio / setRatio: each side keeps 360px; too narrow gives 50 / 50', () => {
  assert.equal(clampRatio(700, 1400), 0.5)
  assert.equal(clampRatio(100, 1400), 360 / 1400)
  assert.equal(clampRatio(1300, 1400), 1 - 360 / 1400)
  assert.equal(clampRatio(300, 700), 0.5)
  assert.equal(clampRatio(NaN, 1400), 0.5)
  assert.deepEqual(ratioLimits(1000), { min: 0.36, max: 0.64 })
  const s = startSplit(three())
  assert.equal(setRatio(s, 840, 1200).split.ratio, 0.7)
  assert.equal(setRatio(s, 1100, 1200).split.ratio, 1 - 360 / 1200)
  assert.equal(resetRatio(setRatio(s, 840, 1200)).split.ratio, 0.5)
  assert.equal(setRatio(three(), 900, 1200).split, undefined)
})

test('effectiveLayout / splitBlockedReason: 1100px threshold, split.on is not changed', () => {
  const s = startSplit(three())
  assert.equal(effectiveLayout(s, 1100), 'split')
  assert.equal(effectiveLayout(s, 1099), 'tabs')
  assert.equal(effectiveLayout(three(), 1920), 'tabs')
  assert.equal(effectiveLayout(stopSplit(s), 1920), 'tabs')
  assert.equal(splitBlockedReason(emptyState(), 1920), 'noFile')
  assert.equal(splitBlockedReason(three(), 1099), 'narrow')
  assert.equal(splitBlockedReason(emptyState(), 900), 'narrow')
  assert.equal(splitBlockedReason(three(), 1100), null)
  assert.equal(fileAreaTab(selectTab(s, CONVERSATION), 'split'), 'file:c.ts')
  assert.equal(fileAreaTab(selectTab(s, CONVERSATION), 'tabs'), CONVERSATION)
})

test('split: survives serialize / parse; a wild ratio is clamped; a split with no tabs is tabs layout', () => {
  const s = setRatio(startSplit(three(), 'file:b.ts'), 840, 1200)
  const back = parse(serialize({ r: s })).r
  assert.deepEqual(back.split, { on: true, ratio: 0.7, rightActive: 'file:b.ts' })
  const wild = parse(JSON.stringify({ r: { tabs: [], split: { on: true, ratio: 9, rightActive: null } } })).r
  assert.equal(wild.split.ratio, 0.9)
  assert.equal(effectiveLayout(wild, 1920), 'tabs')
})

test('renameTab: the right side follows a renamed file', () => {
  const s = renameTab(startSplit(three(), 'file:b.ts'), 'b.ts', 'z.ts')
  assert.equal(s.split.rightActive, 'file:z.ts')
})

test('removeTabsUnder: closes the file and the files under a deleted folder, not look-alikes', () => {
  let s = emptyState()
  s = openTab(s, 'src/x/a.ts', null)
  s = openTab(s, 'src/x/b.ts', null)
  s = openTab(s, 'src/xy.ts', null)
  s = selectTab(s, 'file:src/x/a.ts')
  s = removeTabsUnder(s, 'src/x')
  assert.deepEqual(ids(s), ['file:src/xy.ts'])
  assert.notEqual(s.active, 'file:src/x/a.ts')
  assert.equal(removeTabsUnder(s, 'src/xy.ts').tabs.length, 0)
})

test('removeTabsUnder: nothing covered returns the same state', () => {
  const s = three()
  assert.equal(removeTabsUnder(s, 'zzz'), s)
})

test('moveTab: moves a tab to an index of the shown order; input is not mutated', () => {
  const before = three() // a, b, c
  const s = moveTab(before, 'file:c.ts', 0)
  assert.deepEqual(ids(s), ['file:c.ts', 'file:a.ts', 'file:b.ts'])
  assert.deepEqual(ids(before), ['file:a.ts', 'file:b.ts', 'file:c.ts'])
  assert.deepEqual(ids(moveTab(s, 'file:c.ts', 2)), ['file:a.ts', 'file:b.ts', 'file:c.ts'])
  assert.equal(s.active, before.active)
})

test('moveTab: a tab stays in its group, pinned or not; Conversation, unknown ids and no-ops return the same state', () => {
  const s = togglePin(togglePin(three(), 'file:b.ts'), 'file:c.ts') // b, c (pinned) | a
  assert.deepEqual(ids(moveTab(s, 'file:c.ts', 0)), ['file:c.ts', 'file:b.ts', 'file:a.ts'])
  // Past the pinned group: clamped to its last slot.
  assert.equal(moveTab(s, 'file:c.ts', 2), s)
  assert.deepEqual(ids(moveTab(s, 'file:b.ts', 9)), ['file:c.ts', 'file:b.ts', 'file:a.ts'])
  // An unpinned tab cannot go before the pinned ones.
  assert.equal(moveTab(s, 'file:a.ts', 0), s)
  assert.equal(moveTab(s, CONVERSATION, 1), s)
  assert.equal(moveTab(s, 'file:zz.ts', 0), s)
})

test('moveTab: the new order survives serialize / parse', () => {
  const s = moveTab(three(), 'file:a.ts', 2)
  assert.deepEqual(ids(parse(serialize({ r: s })).r), ['file:b.ts', 'file:c.ts', 'file:a.ts'])
})

// ── RT-13 (#22): one file, one tab ─────────────────────────────────────────

const ROOT = '/Users/me/repo'

test('normalizeTabPath: drops ./, makes a path under the root relative, leaves the rest', () => {
  assert.equal(normalizeTabPath('tasks/x.md', ROOT), 'tasks/x.md')
  assert.equal(normalizeTabPath('./tasks/x.md', ROOT), 'tasks/x.md')
  assert.equal(normalizeTabPath(`${ROOT}/tasks/x.md`, ROOT), 'tasks/x.md')
  assert.equal(normalizeTabPath(`${ROOT}/tasks/x.md`, `${ROOT}/`), 'tasks/x.md')
  // Outside the root (or a sibling with the same prefix) stays absolute.
  assert.equal(normalizeTabPath('/Users/me/other/x.md', ROOT), '/Users/me/other/x.md')
  assert.equal(normalizeTabPath('/Users/me/repo2/x.md', ROOT), '/Users/me/repo2/x.md')
  // No root known: only ./ goes.
  assert.equal(normalizeTabPath(`./${'a.md'}`, null), 'a.md')
  assert.equal(normalizeTabPath(`${ROOT}/a.md`, null), `${ROOT}/a.md`)
  assert.equal(fileTabId('./a.md', ROOT), 'file:a.md')
})

test('openTab with the root: relative, absolute and ./ give one tab', () => {
  let s = emptyState()
  s = openTab(s, 'tasks/#28/x.md', null, 0, ROOT)
  s = openTab(s, `${ROOT}/tasks/#28/x.md`, 12, 0, ROOT)
  s = openTab(s, './tasks/#28/x.md', null, 0, ROOT)
  assert.deepEqual(ids(s), ['file:tasks/#28/x.md'])
  assert.equal(s.tabs[0].path, 'tasks/#28/x.md')
  assert.equal(s.tabs[0].line, 12)
  assert.equal(s.active, 'file:tasks/#28/x.md')
})

test('openTab: a worker worktree under the project resolves against the project root', () => {
  const wt = `${ROOT}/.puppeteer/worktrees/w1`
  let s = openTab(emptyState(), `${wt}/src/a.ts`, null, 0, ROOT)
  s = openTab(s, '.puppeteer/worktrees/w1/src/a.ts', null, 0, ROOT)
  assert.deepEqual(ids(s), ['file:.puppeteer/worktrees/w1/src/a.ts'])
  // A worktree outside the project stays absolute (the viewer reads relative paths from the project).
  const out = openTab(emptyState(), '/tmp/wt/src/a.ts', null, 0, ROOT)
  assert.deepEqual(ids(out), ['file:/tmp/wt/src/a.ts'])
})

test('dedupeTabs: stored duplicates merge into the first, keeping selection, pin and split', () => {
  const raw = {
    tabs: [
      { id: 'file:tasks/x.md', kind: 'file', path: 'tasks/x.md', line: null, pinned: false },
      { id: `file:${ROOT}/tasks/x.md`, kind: 'file', path: `${ROOT}/tasks/x.md`, line: 4, pinned: true },
      { id: 'file:b.md', kind: 'file', path: 'b.md', line: null, pinned: false },
    ],
    active: `file:${ROOT}/tasks/x.md`,
    split: { on: true, ratio: 0.5, rightActive: `file:${ROOT}/tasks/x.md` },
    usedAt: 5,
  }
  const s = dedupeTabs(raw, ROOT)
  assert.deepEqual(s.tabs.map((t) => t.id), ['file:tasks/x.md', 'file:b.md'])
  assert.equal(s.tabs[0].pinned, true)
  assert.equal(s.active, 'file:tasks/x.md')
  assert.equal(s.split.rightActive, 'file:tasks/x.md')
  // Nothing to merge → the same object.
  const clean = openTab(emptyState(), 'a.md', null)
  assert.equal(dedupeTabs(clean, ROOT), clean)
})

test('dedupeTabs under a worker worktree root: its spellings merge, a main checkout tab stays', () => {
  const WT = '/Users/me/repo-wt'
  const raw = {
    tabs: [
      { id: 'file:tasks/x.md', kind: 'file', path: 'tasks/x.md', line: null, pinned: false },
      { id: `file:${WT}/tasks/x.md`, kind: 'file', path: `${WT}/tasks/x.md`, line: 3, pinned: false },
      { id: `file:${ROOT}/tasks/x.md`, kind: 'file', path: `${ROOT}/tasks/x.md`, line: null, pinned: false },
    ],
    active: `file:${ROOT}/tasks/x.md`,
    usedAt: 1,
  }
  const s = dedupeTabs(raw, WT)
  assert.deepEqual(s.tabs.map((t) => t.id), ['file:tasks/x.md', `file:${ROOT}/tasks/x.md`])
  assert.equal(s.tabs[1].path, `${ROOT}/tasks/x.md`)
  assert.equal(s.active, `file:${ROOT}/tasks/x.md`)
})

test('parse: stored ./ duplicates merge on load', () => {
  const stored = serialize({
    r1: {
      tabs: [
        { id: 'file:a.md', kind: 'file', path: 'a.md', line: null, pinned: false },
        { id: 'file:./a.md', kind: 'file', path: './a.md', line: null, pinned: false },
      ],
      active: 'file:./a.md',
      usedAt: 1,
    },
  })
  const s = parse(stored).r1
  assert.deepEqual(s.tabs.map((t) => t.id), ['file:a.md'])
  assert.equal(s.active, 'file:a.md')
})
