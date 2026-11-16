// Unit tests for lib/windowsOverlay.ts: which windows get a card, filter, groups,
// the keyboard selection and the stored values of the "Open windows" overlay.
// Run: node --test src/lib/__tests__/windowsOverlay.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  GROUP_STORAGE_KEY, META_STORAGE_KEY, RECENT_STORAGE_KEY, RUN_STATUS_STORAGE_KEY,
  canMergeBack, cardsFrom, cycleSelection, filterCards, flattenGroups, fold, groupCards,
  initialSelection, kindOfLabel, moveSelection, parseGroupBy, parseMeta, parseRecent,
  parseRunStatuses, runStatusOf, selectionAfterClose, touchRecent,
} from '../windowsOverlay.ts'

const ctx = {
  nameOf: (label, title) => title || label,
  projectNameOf: (id) => ({ p1: 'Puppeteer', p2: 'awog-core' })[id] ?? '',
  screenSub: (label, title) => (label.startsWith('remote-') ? title : 'Màn tách ra'),
}

const win = (label, title = '', minimized = false) => ({ label, title, minimized })

test('storage keys are the ones the issue fixed', () => {
  assert.equal(META_STORAGE_KEY, 'puppeteer.windowMeta')
  assert.ok(RECENT_STORAGE_KEY && RUN_STATUS_STORAGE_KEY && GROUP_STORAGE_KEY)
})

test('kindOfLabel: no card for the main window, the permission prompts and the bridge', () => {
  for (const l of ['main', 'permission-prompt', 'permission-abc', 'remote-bridge', 'remote-permission-abc']) {
    assert.equal(kindOfLabel(l), null, l)
  }
})

test('kindOfLabel: kinds by label', () => {
  assert.equal(kindOfLabel('session-1x'), 'session')
  assert.equal(kindOfLabel('fileviewer-9'), 'file')
  assert.equal(kindOfLabel('secret-notes'), 'secret')
  assert.equal(kindOfLabel('remote-desktop'), 'remote')
  assert.equal(kindOfLabel('remote-calendar'), 'remote')
  assert.equal(kindOfLabel('remote-session-2'), 'remote')
  for (const l of ['gantt', 'calendar', 'pr-inbox', 'settings', 'focus', 'item-list-todo', 'file-transfer']) {
    assert.equal(kindOfLabel(l), 'screen', l)
  }
})

test('canMergeBack: only the screens that have a Move back button', () => {
  for (const l of ['calendar', 'gantt', 'pr-inbox', 'settings']) assert.ok(canMergeBack(l), l)
  for (const l of ['remote-desktop', 'session-1', 'fileviewer-1', 'secret-notes', 'file-transfer']) assert.ok(!canMergeBack(l), l)
})

test('cardsFrom: drops the main window and builds the second lines', () => {
  const cards = cardsFrom(
    [win('main'), win('permission-x'), win('session-a', 'Fix SSO'), win('fileviewer-b', 'refresh.ts', true), win('gantt', 'Gantt'), win('remote-desktop', 'mac-mini')],
    { 'session-a': { projectId: 'p1', runId: 'r1' }, 'fileviewer-b': { projectId: 'p1', folder: 'src/auth' } },
    { r1: 'waiting' },
    'session-a',
    ctx,
  )
  assert.deepEqual(cards.map((c) => c.label), ['fileviewer-b', 'gantt', 'remote-desktop', 'session-a'])
  const byLabel = Object.fromEntries(cards.map((c) => [c.label, c]))
  assert.equal(byLabel['session-a'].sub, 'Puppeteer')
  assert.equal(byLabel['session-a'].status, 'waiting')
  assert.equal(byLabel['session-a'].isHere, true)
  assert.equal(byLabel['fileviewer-b'].sub, 'Puppeteer · src/auth')
  assert.equal(byLabel['fileviewer-b'].minimized, true)
  assert.equal(byLabel.gantt.sub, 'Màn tách ra')
  assert.equal(byLabel['remote-desktop'].sub, 'mac-mini')
  assert.equal(byLabel.gantt.isHere, false)
})

test('cardsFrom: the vault card carries no title, sub-line, project or status (QĐ-18)', () => {
  const [card] = cardsFrom(
    [win('secret-notes', 'My passwords — Puppeteer')],
    { 'secret-notes': { projectId: 'p1', runId: 'r1' } },
    { r1: 'running' },
    'main',
    { ...ctx, nameOf: (label, title) => (title ? `LEAK ${title}` : 'Secret notes') },
  )
  assert.equal(card.kind, 'secret')
  assert.equal(card.title, 'Secret notes')
  assert.equal(card.sub, '')
  assert.equal(card.projectName, undefined)
  assert.equal(card.status, undefined)
})

test('cardsFrom: a session with no known status shows no badge (nothing guessed)', () => {
  const [card] = cardsFrom([win('session-a', 'x')], { 'session-a': { projectId: 'p1', runId: 'r9' } }, {}, 'main', ctx)
  assert.equal(card.status, undefined)
})

test('fold: case and Vietnamese accents are ignored', () => {
  assert.equal(fold('Đăng NHẬP'), 'dang nhap')
})

const CARDS = cardsFrom(
  [
    win('session-a', 'Sửa lỗi đăng nhập SSO'),
    win('session-b', 'Thêm phân trang'),
    win('fileviewer-c', 'refresh.ts'),
    win('gantt', 'Gantt'),
    win('calendar', 'Lịch'),
    win('remote-desktop', 'Puppeteer từ xa'),
  ],
  { 'session-a': { projectId: 'p1', runId: 'r1' }, 'session-b': { projectId: 'p2', runId: 'r2' }, 'fileviewer-c': { projectId: 'p1' } },
  { r2: 'waiting' },
  'main',
  ctx,
)

test('filterCards: "sso" and "dang nhap" match, "zalo" does not, empty keeps all', () => {
  assert.deepEqual(filterCards(CARDS, 'sso').map((c) => c.label), ['session-a'])
  assert.deepEqual(filterCards(CARDS, 'DANG nhap').map((c) => c.label), ['session-a'])
  assert.deepEqual(filterCards(CARDS, 'lich').map((c) => c.label), ['calendar'])
  assert.equal(filterCards(CARDS, 'zalo').length, 0)
  assert.equal(filterCards(CARDS, '  ').length, CARDS.length)
})

test('groupCards by type: fewer cards first, then session, remote, screen', () => {
  const groups = groupCards(CARDS, 'type')
  assert.deepEqual(groups.map((g) => [g.key, g.cards.length]), [['remote', 1], ['session', 2], ['screen', 3]])
  assert.equal(flattenGroups(groups).length, CARDS.length)
})

test('groupCards by project: by name, "no project" last', () => {
  const groups = groupCards(CARDS, 'project')
  assert.deepEqual(groups.map((g) => g.key), ['awog-core', 'Puppeteer', 'none'])
  assert.deepEqual(groups.map((g) => g.isNone), [false, false, true])
  assert.deepEqual(groups[1].cards.map((c) => c.label), ['fileviewer-c', 'session-a'])
})

test('initialSelection: from the main window, the window used last', () => {
  assert.equal(initialSelection(CARDS, 'main', ['gantt', 'session-a']), 'gantt')
  assert.equal(initialSelection(CARDS, 'main', ['gone', 'session-a']), 'session-a')
  assert.equal(initialSelection(CARDS, 'main', []), CARDS[0].label)
  assert.equal(initialSelection([], 'main', []), null)
})

test('initialSelection: from a session window, the session waiting for approval, else the last used other one', () => {
  assert.equal(initialSelection(CARDS, 'session-a', ['gantt']), 'session-b')
  const calm = cardsFrom([win('session-a', 'a'), win('gantt', 'g'), win('calendar', 'c')], {}, {}, 'session-a', ctx)
  assert.equal(initialSelection(calm, 'session-a', ['session-a', 'calendar']), 'calendar')
})

test('initialSelection: the only window is the one it opens on', () => {
  const only = cardsFrom([win('gantt', 'g')], {}, {}, 'gantt', ctx)
  assert.equal(initialSelection(only, 'gantt', []), 'gantt')
})

test('moveSelection: arrows over a grid of 3 columns, ends stay put', () => {
  const six = flattenGroups(groupCards(CARDS, 'project'))
  const at = (sel, dir) => moveSelection(six, sel, dir, 3)
  assert.equal(at(six[0].label, 'right'), six[1].label)
  assert.equal(at(six[0].label, 'left'), six[0].label)
  assert.equal(at(six[0].label, 'down'), six[3].label)
  assert.equal(at(six[4].label, 'up'), six[1].label)
  assert.equal(at(six[4].label, 'down'), six[4].label)
  assert.equal(at(six[5].label, 'right'), six[5].label)
  assert.equal(at(null, 'right'), six[0].label)
  assert.equal(moveSelection([], null, 'right', 3), null)
})

test('cycleSelection: next card, back to the first after the last', () => {
  assert.equal(cycleSelection(CARDS, CARDS[0].label), CARDS[1].label)
  assert.equal(cycleSelection(CARDS, CARDS[CARDS.length - 1].label), CARDS[0].label)
  assert.equal(cycleSelection(CARDS, null), CARDS[0].label)
  assert.equal(cycleSelection([], null), null)
})

test('selectionAfterClose: the next card, else the previous, else none', () => {
  assert.equal(selectionAfterClose(CARDS, CARDS[0].label), CARDS[1].label)
  assert.equal(selectionAfterClose(CARDS, CARDS[CARDS.length - 1].label), CARDS[CARDS.length - 2].label)
  assert.equal(selectionAfterClose([CARDS[0]], CARDS[0].label), null)
  assert.equal(selectionAfterClose(CARDS, 'nope'), null)
})

test('touchRecent: moves to the front without duplicates', () => {
  assert.deepEqual(touchRecent(['a', 'b', 'c'], 'c'), ['c', 'a', 'b'])
  assert.deepEqual(touchRecent([], 'a'), ['a'])
  assert.equal(touchRecent(Array.from({ length: 30 }, (_, i) => `w${i}`), 'new').length, 20)
})

test('parseMeta / parseRecent / parseRunStatuses: broken or foreign values give empty results', () => {
  for (const raw of [null, '', '{', '[]', '"x"']) {
    assert.deepEqual(parseMeta(raw), {})
    assert.deepEqual(parseRunStatuses(raw), {})
  }
  assert.deepEqual(parseRecent('{"a":1}'), [])
  assert.deepEqual(parseMeta('{"s":{"projectId":"p","runId":3,"x":1},"bad":5}'), { s: { projectId: 'p' } })
  assert.deepEqual(parseRecent('["a",1,"b"]'), ['a', 'b'])
  assert.deepEqual(parseRunStatuses('{"r1":"waiting","r2":"done","r3":"error"}'), { r1: 'waiting', r3: 'error' })
})

test('runStatusOf: waiting beats running; failed is an error; the rest has no badge', () => {
  assert.equal(runStatusOf({ status: 'running', pending: 1 }), 'waiting')
  assert.equal(runStatusOf({ status: 'running', pending: 0 }), 'running')
  assert.equal(runStatusOf({ status: 'failed', pending: 0 }), 'error')
  assert.equal(runStatusOf({ status: 'done', pending: 0 }), null)
})

test('parseGroupBy: "project" or the default "type"', () => {
  assert.equal(parseGroupBy('project'), 'project')
  assert.equal(parseGroupBy('type'), 'type')
  assert.equal(parseGroupBy(null), 'type')
  assert.equal(parseGroupBy('x'), 'type')
})
