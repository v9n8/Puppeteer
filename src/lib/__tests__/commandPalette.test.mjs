// Unit tests for lib/commandPalette.ts (⌘K palette: stored layout, items shown,
// accent-insensitive fuzzy search). Run: node --test src/lib/__tests__/commandPalette.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  PALETTE_GROUPS,
  defaultPaletteConfig,
  foldText,
  fuzzyScore,
  hideItem,
  moveGroup,
  paletteSections,
  parsePaletteConfig,
  resetPalettePart,
  serializePaletteConfig,
  setGroupEnabled,
  togglePinned,
  unhideItem,
  visibleItems,
} from '../commandPalette.ts'

const prop = (id, effective) => ({
  id,
  stored: null,
  effective,
  locked: false,
  default: effective,
  degraded: [],
  reason_key: null,
})

// Focus is off; Search is on.
const tree = [
  { module: 'productivity', enabled: true, locked: false, props: [prop('productivity.focus', false)] },
  { module: 'sessions', enabled: true, locked: false, props: [prop('sessions.cross_session_search', true)] },
]

const items = [
  { id: 'go./projects', group: 'nav', label: 'Projects' },
  { id: 'go./focus', group: 'nav', label: 'Focus', feature: 'productivity.focus' },
  { id: 'go./search', group: 'nav', label: 'Search', feature: 'sessions.cross_session_search' },
  { id: 'session:r1', group: 'session', label: 'Sửa lỗi đăng nhập', detail: 'Puppeteer' },
  { id: 'project:p1', group: 'project', label: 'Puppeteer', detail: '/Users/me/Puppeteer' },
  { id: 'prompt:a', group: 'prompt', label: 'Review code', detail: 'Review the diff' },
  { id: 'settings.general', group: 'settings', label: 'Cài đặt chung' },
]

const ids = (sections) => sections.map((s) => [s.id, s.items.map((i) => i.id)])

test('parsePaletteConfig: missing or malformed values give the defaults', () => {
  const defaults = defaultPaletteConfig()
  assert.deepEqual(defaults.groups.map((g) => g.id), [...PALETTE_GROUPS])
  assert.ok(defaults.groups.every((g) => g.enabled))
  for (const raw of [undefined, null, '', 'not json', '[]', '42', 'null']) {
    assert.deepEqual(parsePaletteConfig(raw), defaults, String(raw))
  }
})

test('parsePaletteConfig: keeps a stored order, drops unknown groups, appends new ones', () => {
  const config = parsePaletteConfig(JSON.stringify({
    groups: [
      { id: 'prompt', enabled: false },
      { id: 'nope', enabled: true },
      { id: 'nav' },
      { id: 'prompt', enabled: true },
      'session',
    ],
    pinned: ['go./skills', 'go./skills', '', 7, 'project:p1'],
    hidden: ['project:p1', 'session:x'],
  }))
  assert.deepEqual(config.groups, [
    { id: 'prompt', enabled: false },
    { id: 'nav', enabled: true },
    { id: 'session', enabled: true },
    { id: 'project', enabled: true },
    { id: 'settings', enabled: true },
  ])
  // Deduplicated, and a hidden item is never pinned.
  assert.deepEqual(config.pinned, ['go./skills'])
  assert.deepEqual(config.hidden, ['project:p1', 'session:x'])
})

test('serializePaletteConfig round-trips', () => {
  let config = defaultPaletteConfig()
  config = moveGroup(config, 'settings', -10)
  config = setGroupEnabled(config, 'session', false)
  config = togglePinned(config, 'go./skills')
  config = hideItem(config, 'project:p1')
  assert.deepEqual(parsePaletteConfig(serializePaletteConfig(config)), config)
})

test('moveGroup reorders and clamps; setGroupEnabled flips one group', () => {
  const start = defaultPaletteConfig()
  assert.deepEqual(moveGroup(start, 'project', -1).groups.map((g) => g.id), ['nav', 'project', 'session', 'prompt', 'settings'])
  assert.deepEqual(moveGroup(start, 'nav', 99).groups.map((g) => g.id), ['session', 'project', 'prompt', 'settings', 'nav'])
  assert.deepEqual(moveGroup(start, 'nav', -1).groups.map((g) => g.id), [...PALETTE_GROUPS])
  const off = setGroupEnabled(start, 'prompt', false)
  assert.deepEqual(off.groups.filter((g) => !g.enabled).map((g) => g.id), ['prompt'])
  assert.ok(start.groups.every((g) => g.enabled), 'not mutated')
})

test('togglePinned / hideItem / unhideItem work by id', () => {
  let config = defaultPaletteConfig()
  config = togglePinned(config, 'go./skills')
  config = togglePinned(config, 'project:p1')
  assert.deepEqual(config.pinned, ['go./skills', 'project:p1'])
  config = togglePinned(config, 'go./skills')
  assert.deepEqual(config.pinned, ['project:p1'])
  config = hideItem(config, 'project:p1')
  assert.deepEqual(config.pinned, [], 'hiding unpins')
  assert.deepEqual(config.hidden, ['project:p1'])
  assert.equal(hideItem(config, 'project:p1'), config, 'hiding twice is a no-op')
  config = unhideItem(config, 'project:p1')
  assert.deepEqual(config.hidden, [])
  assert.deepEqual(config.pinned, [], 'unhiding does not re-pin')
})

test('resetPalettePart restores one part and keeps the others', () => {
  let config = defaultPaletteConfig()
  config = moveGroup(config, 'settings', -4)
  config = setGroupEnabled(config, 'nav', false)
  config = togglePinned(config, 'go./skills')
  config = hideItem(config, 'project:p1')

  const groups = resetPalettePart(config, 'groups')
  assert.deepEqual(groups.groups, defaultPaletteConfig().groups)
  assert.deepEqual([groups.pinned, groups.hidden], [['go./skills'], ['project:p1']])

  const pinned = resetPalettePart(config, 'pinned')
  assert.deepEqual(pinned.pinned, [])
  assert.deepEqual([pinned.groups, pinned.hidden], [config.groups, config.hidden])

  const hidden = resetPalettePart(config, 'hidden')
  assert.deepEqual(hidden.hidden, [])
  assert.deepEqual([hidden.groups, hidden.pinned], [config.groups, config.pinned])
})

test('visibleItems: an item whose prop is off is gone, even pinned', () => {
  let config = togglePinned(defaultPaletteConfig(), 'go./focus')
  config = togglePinned(config, 'go./search')
  assert.deepEqual(
    visibleItems(items, config, tree).map((i) => i.id),
    ['go./projects', 'go./search', 'session:r1', 'project:p1', 'prompt:a', 'settings.general'],
  )
  assert.deepEqual(ids(paletteSections(items, config, tree, '')).slice(0, 2), [
    ['pinned', ['go./search']],
    ['nav', ['go./projects']],
  ])
  // Unknown to the tree (feature not loaded yet): off.
  assert.deepEqual(visibleItems(items, config, []).map((i) => i.id).includes('go./search'), false)
})

test('paletteSections: pinned first, then the groups in their order; off groups and hidden items left out', () => {
  let config = defaultPaletteConfig()
  config = moveGroup(config, 'settings', -4)
  config = setGroupEnabled(config, 'session', false)
  config = hideItem(config, 'prompt:a')
  config = togglePinned(config, 'project:p1')
  config = togglePinned(config, 'settings.general')
  config = togglePinned(config, 'session:r1') // its group is off: not listed even pinned
  assert.deepEqual(ids(paletteSections(items, config, tree, '')), [
    ['pinned', ['project:p1', 'settings.general']],
    ['nav', ['go./projects', 'go./search']],
  ])
})

test('paletteSections: a query keeps the matches, best first, in every section', () => {
  const config = togglePinned(defaultPaletteConfig(), 'project:p1')
  // "puppeteer" names the project and is the session's detail.
  assert.deepEqual(ids(paletteSections(items, config, tree, 'puppeteer')), [
    ['pinned', ['project:p1']],
    ['session', ['session:r1']],
  ])
  // Accents ignored, word order free.
  assert.deepEqual(ids(paletteSections(items, config, tree, 'dang nhap sua')), [['session', ['session:r1']]])
  assert.deepEqual(ids(paletteSections(items, config, tree, 'cai dat')), [['settings', ['settings.general']]])
  assert.deepEqual(paletteSections(items, config, tree, 'zzzz'), [])
})

test('foldText drops Vietnamese accents, đ included', () => {
  assert.equal(foldText('Cài Đặt chung'), 'cai dat chung')
  assert.equal(foldText('Sửa lỗi ở được'), 'sua loi o duoc')
  assert.equal(foldText('Tiếng Việt'), 'tieng viet')
})

test('fuzzyScore: substrings beat scattered letters; word starts and early hits rank higher', () => {
  assert.equal(fuzzyScore('', 'anything'), 0)
  assert.equal(fuzzyScore('xyz', 'Projects'), null)
  assert.equal(fuzzyScore('sproj', 'Projects'), null, 'letters out of order')
  assert.ok(fuzzyScore('nss', 'New session') !== null, 'letters in order')
  assert.ok(fuzzyScore('sess', 'New session') > fuzzyScore('nss', 'New session'))
  assert.ok(fuzzyScore('set', 'Settings') > fuzzyScore('set', 'Reset'), 'word start')
  assert.ok(fuzzyScore('cai', 'Cài đặt') > 0, 'accents ignored in the text')
  assert.ok(fuzzyScore('cài', 'Cai dat') > 0, 'and in the query')
  assert.equal(fuzzyScore('review missing', 'Review code'), null, 'every word must match')
})
