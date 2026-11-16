// Unit tests for the pure parts of lib/appearanceSync.ts (theme / palette /
// language changed in one window, applied in every window).
// Run: node --test src/lib/__tests__/appearanceSync.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { applyAppearance, readAppearanceChange } from '../appearanceSync.ts'

function fakeRoot(dark = false, theme = null) {
  const root = {
    dark,
    theme,
    classList: {
      toggle(token, force) {
        assert.equal(token, 'dark')
        root.dark = force
        return force
      },
    },
    setAttribute(name, value) {
      assert.equal(name, 'data-theme')
      root.theme = value
    },
    removeAttribute(name) {
      assert.equal(name, 'data-theme')
      root.theme = null
    },
  }
  return root
}

test('readAppearanceChange keeps string fields only', () => {
  assert.deepEqual(readAppearanceChange({ theme: 'dark', colorTheme: 'nord', language: 'vi' }), {
    theme: 'dark', colorTheme: 'nord', language: 'vi',
  })
  assert.deepEqual(readAppearanceChange({ theme: 1, language: null, other: 'x' }), {})
  assert.deepEqual(readAppearanceChange(null), {})
  assert.deepEqual(readAppearanceChange('dark'), {})
})

test('applyAppearance: light / dark / system mode', () => {
  const root = fakeRoot(true)
  applyAppearance({ theme: 'light' }, root, true)
  assert.equal(root.dark, false)
  applyAppearance({ theme: 'dark' }, root, false)
  assert.equal(root.dark, true)
  applyAppearance({ theme: 'system' }, root, false)
  assert.equal(root.dark, false)
  applyAppearance({ theme: 'system' }, root, true)
  assert.equal(root.dark, true)
})

test('applyAppearance: palette attribute, default and unknown ids clear it', () => {
  const root = fakeRoot(false, 'ocean')
  applyAppearance({ colorTheme: 'nord' }, root, false)
  assert.equal(root.theme, 'nord')
  applyAppearance({ colorTheme: 'default' }, root, false)
  assert.equal(root.theme, null)
  root.theme = 'nord'
  applyAppearance({ colorTheme: 'no-such-theme' }, root, false)
  assert.equal(root.theme, null)
})

test('applyAppearance: fields left out are left alone', () => {
  const root = fakeRoot(true, 'nord')
  applyAppearance({ language: 'vi' }, root, false)
  assert.equal(root.dark, true)
  assert.equal(root.theme, 'nord')
})
