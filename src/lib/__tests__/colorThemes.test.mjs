// Unit tests for lib/colorThemes.ts (palette registry) — every registered id
// has CSS and an i18n name in both locales.
// Run: node --test src/lib/__tests__/colorThemes.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { COLOR_THEMES, colorThemeHintKey, colorThemeLabelKey, knownColorTheme, normalizeColorTheme } from '../colorThemes.ts'

const src = fileURLToPath(new URL('../../', import.meta.url))
const mainCss = readFileSync(`${src}assets/main.css`, 'utf8')

test('ids are unique and default comes first', () => {
  assert.equal(new Set(COLOR_THEMES).size, COLOR_THEMES.length)
  assert.equal(COLOR_THEMES[0], 'default')
})

test('every named theme has light and dark CSS blocks', () => {
  for (const id of COLOR_THEMES.filter((t) => t !== 'default')) {
    const file = `${src}assets/themes/${id}.css`
    assert.ok(existsSync(file), `${id} has no file`)
    const css = readFileSync(file, 'utf8')
    assert.ok(css.includes(`[data-theme='${id}']:not(.dark)`), `${id} light`)
    assert.ok(css.includes(`[data-theme='${id}'].dark`), `${id} dark`)
    assert.ok(mainCss.includes(`@import "./themes/${id}.css";`), `${id} imported`)
  }
})

test('every theme has a name and a description in en and vi', () => {
  for (const loc of ['en', 'vi']) {
    const general = JSON.parse(readFileSync(`${src}i18n/locales/${loc}/settings.json`, 'utf8')).general
    for (const id of COLOR_THEMES) {
      for (const key of [colorThemeLabelKey(id), colorThemeHintKey(id)]) {
        const leaf = key.replace('settings.general.', '')
        assert.equal(typeof general[leaf], 'string', `${loc} ${leaf}`)
      }
    }
  }
})

test('no theme file is left without a registered id', () => {
  const imported = [...mainCss.matchAll(/@import "\.\/themes\/([\w-]+)\.css";/g)].map((m) => m[1])
  assert.deepEqual(imported.sort(), COLOR_THEMES.filter((t) => t !== 'default').sort())
})

test('removed palettes read as their replacement, unknown ids as default', () => {
  assert.equal(normalizeColorTheme('macos'), 'ocean')
  assert.equal(normalizeColorTheme('tahoe'), 'ocean')
  assert.equal(normalizeColorTheme('crt'), 'contrast')
  assert.equal(normalizeColorTheme('monokai'), 'tokyonight')
  assert.equal(normalizeColorTheme('midautumn'), 'default')
  assert.equal(normalizeColorTheme('paper'), 'paper')
  assert.equal(normalizeColorTheme('neon'), 'default')
  assert.equal(normalizeColorTheme(undefined), 'default')
  assert.equal(knownColorTheme('neon'), null)
})

test('label key capitalizes the id', () => {
  assert.equal(colorThemeLabelKey('tokyonight'), 'settings.general.colorTokyonight')
})
