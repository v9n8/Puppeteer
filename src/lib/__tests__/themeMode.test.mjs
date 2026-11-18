import test from 'node:test'
import assert from 'node:assert/strict'
import { resolveDark, applyThemeMode } from '../themeMode.ts'

function fakeRoot() {
  const classes = new Set()
  return {
    classes,
    classList: {
      toggle(t, force) {
        const on = force ?? !classes.has(t)
        if (on) classes.add(t)
        else classes.delete(t)
        return on
      },
    },
  }
}

function fakeMedia(matches) {
  const listeners = new Set()
  return {
    matches,
    listeners,
    addEventListener: (_t, l) => listeners.add(l),
    removeEventListener: (_t, l) => listeners.delete(l),
    fire(m) {
      this.matches = m
      for (const l of [...listeners]) l({ matches: m })
    },
  }
}

test('resolveDark: explicit values ignore the OS', () => {
  for (const p of [true, false]) {
    assert.equal(resolveDark('dark', p), true)
    assert.equal(resolveDark('light', p), false)
  }
})

test('resolveDark: system and unknown values follow the OS', () => {
  for (const t of ['system', 'bogus', '']) {
    assert.equal(resolveDark(t, true), true)
    assert.equal(resolveDark(t, false), false)
  }
})

test('applyThemeMode dark / light set the class and add no listener', () => {
  const root = fakeRoot()
  const media = fakeMedia(false)
  applyThemeMode('dark', root, media)
  assert.ok(root.classes.has('dark'))
  applyThemeMode('light', root, media)
  assert.ok(!root.classes.has('dark'))
  assert.equal(media.listeners.size, 0)
})

test('applyThemeMode system follows the OS and reports through onChange', () => {
  const root = fakeRoot()
  const media = fakeMedia(true)
  const seen = []
  applyThemeMode('system', root, media, (d) => seen.push(d))
  assert.ok(root.classes.has('dark'))
  media.fire(false)
  assert.ok(!root.classes.has('dark'))
  media.fire(true)
  assert.deepEqual(seen, [true, false, true])
})

test('applyThemeMode: the returned function removes the one listener', () => {
  const root = fakeRoot()
  const media = fakeMedia(false)
  const off = applyThemeMode('system', root, media)
  assert.equal(media.listeners.size, 1)
  off()
  assert.equal(media.listeners.size, 0)
  media.fire(true)
  assert.ok(!root.classes.has('dark'))
})

test('applyThemeMode: unknown theme is system', () => {
  const root = fakeRoot()
  const media = fakeMedia(true)
  applyThemeMode('bogus', root, media)
  assert.ok(root.classes.has('dark'))
  assert.equal(media.listeners.size, 1)
})
