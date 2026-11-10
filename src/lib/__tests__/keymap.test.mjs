// Unit tests for lib/keymap.ts (shortcuts of the native menu's actions: syntax,
// system keys refused, duplicates). Run: node --test src/lib/__tests__/keymap.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  DEFAULT_KEYMAP,
  NEW_SESSION_ACTION,
  PALETTE_ACTION,
  PREVIOUS_PROJECT_ACTION,
  acceleratorFromEvent,
  acceleratorProblem,
  formatAccelerator,
  keymapConflict,
  matchesAccelerator,
  normalizeAccelerator,
  parseKeymap,
  resetBinding,
  resolveKeymap,
  serializeKeymap,
  setBinding,
} from '../keymap.ts'

const press = (code, mods = {}) => ({ code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods })

test('normalizeAccelerator: one canonical spelling Tauri accepts', () => {
  assert.equal(normalizeAccelerator('shift+cmdorctrl+k'), 'CmdOrCtrl+Shift+K')
  assert.equal(normalizeAccelerator('CommandOrControl + Option + p'), 'CmdOrCtrl+Alt+P')
  assert.equal(normalizeAccelerator('Control+CmdOrCtrl+1'), 'CmdOrCtrl+Ctrl+1')
  assert.equal(normalizeAccelerator('CmdOrCtrl+,'), 'CmdOrCtrl+,')
  assert.equal(normalizeAccelerator('f5'), 'F5')
  assert.equal(normalizeAccelerator('CmdOrCtrl+Shift+CmdOrCtrl+K'), 'CmdOrCtrl+Shift+K', 'a repeated modifier counts once')
  for (const bad of ['', '+', 'CmdOrCtrl+', 'CmdOrCtrl++', 'Cmd+K', 'Super+K', 'Meta+K', 'CmdOrCtrl+Enter',
    'CmdOrCtrl+Up', 'CmdOrCtrl+Space', 'CmdOrCtrl+KK', 'CmdOrCtrl+F25', 'Hyper+K']) {
    assert.equal(normalizeAccelerator(bad), null, bad)
  }
})

test('acceleratorFromEvent: ⌘ is CmdOrCtrl on macOS, Ctrl elsewhere; physical key', () => {
  assert.equal(acceleratorFromEvent(press('KeyK', { metaKey: true }), true), 'CmdOrCtrl+K')
  assert.equal(acceleratorFromEvent(press('KeyK', { ctrlKey: true }), false), 'CmdOrCtrl+K')
  assert.equal(acceleratorFromEvent(press('KeyK', { ctrlKey: true, metaKey: true, altKey: true, shiftKey: true }), true),
    'CmdOrCtrl+Ctrl+Alt+Shift+K')
  assert.equal(acceleratorFromEvent(press('Digit1', { metaKey: true, shiftKey: true }), true), 'CmdOrCtrl+Shift+1')
  assert.equal(acceleratorFromEvent(press('Comma', { metaKey: true }), true), 'CmdOrCtrl+,')
  assert.equal(acceleratorFromEvent(press('Backquote', { ctrlKey: true }), false), 'CmdOrCtrl+`')
  assert.equal(acceleratorFromEvent(press('F12'), true), 'F12')
  assert.equal(acceleratorFromEvent(press('KeyK', { metaKey: true }), false), null, 'the Windows key is the system\'s')
  for (const code of ['MetaLeft', 'ShiftLeft', 'ArrowUp', 'Enter', 'Space', 'Tab', 'Escape', 'Numpad1', 'IntlBackslash']) {
    assert.equal(acceleratorFromEvent(press(code, { metaKey: true }), true), null, code)
  }
})

test('acceleratorProblem: ⌘Q / ⌘W / ⌘M and the other system keys are refused', () => {
  for (const isMac of [true, false]) {
    for (const accel of ['CmdOrCtrl+Q', 'CmdOrCtrl+W', 'CmdOrCtrl+M', 'CmdOrCtrl+C', 'CmdOrCtrl+V', 'CmdOrCtrl+X',
      'CmdOrCtrl+Z', 'CmdOrCtrl+Shift+Z', 'CmdOrCtrl+A', 'CmdOrCtrl+F', 'CmdOrCtrl+S']) {
      assert.equal(acceleratorProblem(accel, isMac), 'reserved', `${accel} ${isMac}`)
    }
  }
  assert.equal(acceleratorProblem('CmdOrCtrl+H', true), 'reserved', 'Hide')
  assert.equal(acceleratorProblem('CmdOrCtrl+Alt+H', true), 'reserved', 'Hide Others')
  assert.equal(acceleratorProblem('CmdOrCtrl+Ctrl+F', true), 'reserved', 'full screen')
  assert.equal(acceleratorProblem('CmdOrCtrl+Shift+4', true), 'reserved', 'screenshot')
  assert.equal(acceleratorProblem('CmdOrCtrl+H', false), null)
  assert.equal(acceleratorProblem('CmdOrCtrl+Y', false), 'reserved', 'Redo')
  assert.equal(acceleratorProblem('Alt+F4', false), 'reserved', 'close window')
  assert.equal(acceleratorProblem('CmdOrCtrl+Y', true), null)
  // Off macOS Ctrl is the command key: Ctrl+Q is ⌘Q's twin.
  assert.equal(acceleratorProblem('Ctrl+Q', false), 'reserved')
  assert.equal(acceleratorProblem('Ctrl+Q', true), 'needsModifier')
})

test('acceleratorProblem: a key that would fire while typing needs ⌘ / Ctrl', () => {
  for (const accel of ['K', 'Shift+K', 'Alt+K', 'Alt+Shift+1']) {
    assert.equal(acceleratorProblem(accel, true), 'needsModifier', accel)
    assert.equal(acceleratorProblem(accel, false), 'needsModifier', accel)
  }
  assert.equal(acceleratorProblem('Ctrl+K', true), 'needsModifier', '⌃K edits text on macOS')
  assert.equal(acceleratorProblem('Ctrl+K', false), null)
  assert.equal(acceleratorProblem('F5', true), null, 'a function key alone is fine')
  assert.equal(acceleratorProblem('CmdOrCtrl+Shift+K', true), null)
  assert.equal(acceleratorProblem('CmdOrCtrl+Enter', true), 'invalid')
})

test('formatAccelerator: macOS symbols in Apple order, words elsewhere', () => {
  assert.equal(formatAccelerator('CmdOrCtrl+K', true), '⌘K')
  assert.equal(formatAccelerator('CmdOrCtrl+Ctrl+Alt+Shift+K', true), '⌃⌥⇧⌘K')
  assert.equal(formatAccelerator('CmdOrCtrl+,', true), '⌘,')
  assert.equal(formatAccelerator('CmdOrCtrl+Alt+Shift+K', false), 'Ctrl+Alt+Shift+K')
  assert.equal(formatAccelerator('CmdOrCtrl+Ctrl+K', false), 'Ctrl+K')
  assert.equal(formatAccelerator('F5', false), 'F5')
})

test('parseKeymap: never throws, keeps "" (no shortcut), drops what Tauri would refuse', () => {
  for (const raw of [null, undefined, '', 'not json', '[]', '"x"', '42', 'null']) {
    assert.deepEqual(parseKeymap(raw), {}, String(raw))
  }
  assert.deepEqual(
    parseKeymap(JSON.stringify({
      'go./skills': 'shift+cmdorctrl+j',
      'view.commandPalette': '',
      'help.reload': 'Cmd+R',
      'help.docs': 42,
      '': 'CmdOrCtrl+J',
    })),
    { 'go./skills': 'CmdOrCtrl+Shift+J', 'view.commandPalette': '' },
  )
})

test('setBinding / resetBinding: only the changes are stored', () => {
  let keymap = setBinding({}, 'go./skills', 'CmdOrCtrl+J')
  assert.deepEqual(keymap, { 'go./skills': 'CmdOrCtrl+J' })
  keymap = setBinding(keymap, PALETTE_ACTION, '')
  assert.deepEqual(keymap, { 'go./skills': 'CmdOrCtrl+J', [PALETTE_ACTION]: '' })
  keymap = setBinding(keymap, PALETTE_ACTION, DEFAULT_KEYMAP[PALETTE_ACTION])
  assert.deepEqual(keymap, { 'go./skills': 'CmdOrCtrl+J' }, 'back to the default: entry gone')
  assert.deepEqual(setBinding(keymap, 'help.docs', ''), keymap, 'no shortcut is that action\'s default')
  assert.deepEqual(resetBinding(keymap, 'go./skills'), {})
  assert.deepEqual(parseKeymap(serializeKeymap(keymap)), keymap)
})

const ACTIONS = ['go./settings', 'go./skills', 'go./projects', PALETTE_ACTION, 'help.reload']

test('resolveKeymap: defaults, changes, "" removes; system keys never reach the menu', () => {
  assert.deepEqual(Object.fromEntries(resolveKeymap(ACTIONS, {}, true)), {
    'go./settings': 'CmdOrCtrl+,',
    [PALETTE_ACTION]: 'CmdOrCtrl+K',
  })
  assert.deepEqual(
    Object.fromEntries(resolveKeymap(ACTIONS, { [PALETTE_ACTION]: 'CmdOrCtrl+Shift+P', 'go./settings': '', 'help.reload': 'CmdOrCtrl+Q' }, true)),
    { [PALETTE_ACTION]: 'CmdOrCtrl+Shift+P' },
  )
  assert.equal(resolveKeymap(['go./skills'], {}, true).size, 0, 'an action without a default has none')
  assert.equal(resolveKeymap(['help.docs'], { [PALETTE_ACTION]: 'CmdOrCtrl+J' }, true).size, 0,
    'an action not in the menu (feature off) gets nothing')
})

test('resolveKeymap: one set of keys, one action — a change beats a default, then menu order', () => {
  // A hand edit or a backup gave ⌘K to Skills while the palette keeps its default.
  const resolved = resolveKeymap(ACTIONS, { 'go./skills': 'CmdOrCtrl+K' }, true)
  assert.equal(resolved.get('go./skills'), 'CmdOrCtrl+K')
  assert.equal(resolved.has(PALETTE_ACTION), false)
  const both = resolveKeymap(ACTIONS, { 'go./skills': 'CmdOrCtrl+J', 'go./projects': 'CmdOrCtrl+J' }, true)
  assert.equal(both.get('go./skills'), 'CmdOrCtrl+J')
  assert.equal(both.has('go./projects'), false)
  // Same keys off macOS only: ⌃⌘J and ⌘J differ on a Mac.
  const keymap = { 'go./skills': 'CmdOrCtrl+J', 'go./projects': 'CmdOrCtrl+Ctrl+J' }
  assert.equal(resolveKeymap(ACTIONS, keymap, true).size, 4)
  assert.equal(resolveKeymap(ACTIONS, keymap, false).has('go./projects'), false)
})

test('defaults: every one usable on both platforms, no two on the same keys (UX-09 ⌘T, ⌥⌘`)', () => {
  const ids = Object.keys(DEFAULT_KEYMAP)
  for (const isMac of [true, false]) {
    for (const id of ids) assert.equal(acceleratorProblem(DEFAULT_KEYMAP[id], isMac), null, `${id} on ${isMac ? 'mac' : 'other'}`)
    assert.equal(resolveKeymap(ids, {}, isMac).size, ids.length, 'none dropped as a duplicate')
  }
  assert.equal(DEFAULT_KEYMAP[NEW_SESSION_ACTION], 'CmdOrCtrl+T')
  assert.equal(DEFAULT_KEYMAP[PREVIOUS_PROJECT_ACTION], 'CmdOrCtrl+Alt+`')
  // AWOG's ⌘` stays the macOS window cycler, so it can't be the default there.
  assert.equal(acceleratorProblem('CmdOrCtrl+`', true), 'reserved')
})

test('keymapConflict: names the action already on those keys', () => {
  const resolved = resolveKeymap(ACTIONS, { 'go./skills': 'CmdOrCtrl+J' }, true)
  assert.equal(keymapConflict(resolved, 'go./projects', 'CmdOrCtrl+K', true), PALETTE_ACTION)
  assert.equal(keymapConflict(resolved, 'go./projects', 'CmdOrCtrl+J', true), 'go./skills')
  assert.equal(keymapConflict(resolved, 'go./skills', 'CmdOrCtrl+J', true), null, 'its own keys')
  assert.equal(keymapConflict(resolved, 'go./projects', 'CmdOrCtrl+Shift+J', true), null)
  assert.equal(keymapConflict(resolved, 'go./projects', 'Ctrl+K', false), PALETTE_ACTION, 'Ctrl is ⌘ off macOS')
})

test('matchesAccelerator: the key press of a shortcut, for a window without the menu', () => {
  assert.ok(matchesAccelerator(press('KeyK', { metaKey: true }), 'CmdOrCtrl+K', true))
  assert.ok(matchesAccelerator(press('KeyK', { ctrlKey: true }), 'CmdOrCtrl+K', false))
  assert.ok(!matchesAccelerator(press('KeyK', { ctrlKey: true }), 'CmdOrCtrl+K', true), '⌃K edits text on macOS')
  assert.ok(!matchesAccelerator(press('KeyK', { metaKey: true, shiftKey: true }), 'CmdOrCtrl+K', true))
  assert.ok(!matchesAccelerator(press('KeyK', { metaKey: true, altKey: true }), 'CmdOrCtrl+K', true))
  assert.ok(!matchesAccelerator(press('KeyJ', { metaKey: true }), 'CmdOrCtrl+K', true))
  assert.ok(matchesAccelerator(press('KeyP', { metaKey: true, shiftKey: true }), 'CmdOrCtrl+Shift+P', true))
})
