// Unit tests for lib/appFonts.ts (font family and text-size preferences).
// Run: node --test src/lib/__tests__/appFonts.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  DEFAULT_FONT_PREFS, FONT_FAMILIES, fontCssVars, fontStack, parseFontPrefs, pickSize, APP_FONT_SIZES,
} from '../appFonts.ts'

test('the defaults change nothing: theme font, no scale variables', () => {
  assert.deepEqual(fontCssVars(DEFAULT_FONT_PREFS), {
    '--font-ui': null, '--app-text-scale': null, '--chat-text-scale': null,
  })
})

test('sizes become scales against the defaults (13 px app, 14 px conversation)', () => {
  const vars = fontCssVars({ ...DEFAULT_FONT_PREFS, appSize: 15, chatSize: 16 })
  assert.equal(vars['--app-text-scale'], '1.1538')
  assert.equal(vars['--chat-text-scale'], '1.1429')
})

test('fontStack: presets, and a custom name quoted and cleaned with Inter behind it', () => {
  assert.equal(fontStack('theme', ''), null)
  assert.match(fontStack('system', ''), /^system-ui/)
  assert.match(fontStack('custom', 'Avenir Next'), /^"Avenir Next", "Inter"/)
  assert.match(fontStack('custom', 'x"; color: red; {'), /^"x color: red", "Inter"/)
  assert.equal(fontStack('custom', '  '), null)
})

test('parseFontPrefs: missing, corrupt or unknown values take their defaults', () => {
  assert.deepEqual(parseFontPrefs(null), DEFAULT_FONT_PREFS)
  assert.deepEqual(parseFontPrefs('{not json'), DEFAULT_FONT_PREFS)
  assert.deepEqual(parseFontPrefs('{"family":"comic","appSize":99,"chatSize":"16","customFont":3}'), {
    ...DEFAULT_FONT_PREFS, chatSize: 16,
  })
  assert.deepEqual(parseFontPrefs('{"family":"custom","customFont":"Avenir"}'), {
    ...DEFAULT_FONT_PREFS, family: 'custom', customFont: 'Avenir',
  })
})

test('pickSize: only listed sizes', () => {
  assert.equal(pickSize('14', APP_FONT_SIZES, 13), 14)
  assert.equal(pickSize('10', APP_FONT_SIZES, 13), 13)
})

test('every font choice has a label in en and vi', () => {
  const keys = { theme: 'fontTheme', inter: 'fontInter', system: 'fontSystem', helvetica: 'fontHelvetica', noto: 'fontNoto', custom: 'fontCustom' }
  for (const locale of ['en', 'vi']) {
    const general = JSON.parse(readFileSync(new URL(`../../i18n/locales/${locale}/settings.json`, import.meta.url), 'utf8')).general
    for (const f of FONT_FAMILIES) assert.ok(general[keys[f]], `${locale}: ${f}`)
  }
})
