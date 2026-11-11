// Unit tests for lib/appFonts.ts (font family, weight, smoothing, code font and text-size preferences).
// Run: node --test src/lib/__tests__/appFonts.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  APP_FONT_SIZES, CHAT_FONT_SIZES, DEFAULT_FONT_PREFS, FONT_FAMILIES, MONO_FONTS,
  fontCssVars, fontStack, parseFontPrefs, pickSize, resolveSmoothing, supportsWeight,
} from '../appFonts.ts'

test('the defaults change nothing: stylesheet fonts, no weight or scale variables', () => {
  for (const v of Object.values(fontCssVars(DEFAULT_FONT_PREFS))) assert.equal(v, null)
})

test('sizes become scales against the defaults (13 px app, 14 px conversation)', () => {
  const vars = fontCssVars({ ...DEFAULT_FONT_PREFS, appSize: 15, chatSize: 16 })
  assert.equal(vars['--app-text-scale'], '1.1538')
  assert.equal(vars['--chat-text-scale'], '1.1429')
})

test('a weight moves the whole scale by the same step', () => {
  const vars = fontCssVars({ ...DEFAULT_FONT_PREFS, weight: 450 })
  assert.equal(vars['--font-weight-normal'], '450')
  assert.equal(vars['--font-weight-medium'], '550')
  assert.equal(vars['--font-weight-semibold'], '650')
  assert.equal(vars['--font-weight-bold'], '750')
})

test('a code font other than the default sets --font-mono', () => {
  assert.match(fontCssVars({ ...DEFAULT_FONT_PREFS, mono: 'plexmono' })['--font-mono'], /^"IBM Plex Mono"/)
  assert.match(fontCssVars({ ...DEFAULT_FONT_PREFS, mono: 'system' })['--font-mono'], /^ui-monospace/)
})

test('fontStack: presets, and a custom name quoted and cleaned with Inter behind it', () => {
  assert.equal(fontStack('inter', ''), null)
  assert.match(fontStack('system', ''), /^system-ui/)
  assert.match(fontStack('bevietnam', ''), /^"Be Vietnam Pro", "Inter"/)
  assert.match(fontStack('custom', 'Avenir Next'), /^"Avenir Next", "Inter"/)
  assert.match(fontStack('custom', 'x"; color: red; {'), /^"x color: red", "Inter"/)
  assert.equal(fontStack('custom', '  '), null)
})

test('parseFontPrefs: missing, corrupt or unknown values take their defaults', () => {
  assert.deepEqual(parseFontPrefs(null), DEFAULT_FONT_PREFS)
  assert.deepEqual(parseFontPrefs('{not json'), DEFAULT_FONT_PREFS)
  assert.deepEqual(parseFontPrefs('{"family":"comic","appSize":99,"chatSize":"16","customFont":3,"weight":300,"smoothing":"x","mono":"y"}'), {
    ...DEFAULT_FONT_PREFS, chatSize: 16,
  })
  assert.deepEqual(parseFontPrefs('{"family":"custom","customFont":"Avenir"}'), {
    ...DEFAULT_FONT_PREFS, family: 'custom', customFont: 'Avenir',
  })
})

test('parseFontPrefs: choices of earlier versions map to what replaced them', () => {
  assert.equal(parseFontPrefs('{"family":"theme"}').family, 'inter')
  assert.equal(parseFontPrefs('{"family":"helvetica"}').family, 'system')
  // 11 and 12 px are gone: they rise to the 13 px floor instead of jumping to the default.
  assert.equal(parseFontPrefs('{"appSize":11,"chatSize":12}').appSize, 13)
  assert.equal(parseFontPrefs('{"appSize":11,"chatSize":12}').chatSize, 13)
})

test('a font with fixed weights never keeps the in-between weight', () => {
  assert.equal(supportsWeight('bevietnam', 450), false)
  assert.equal(supportsWeight('bevietnam', 500), true)
  assert.equal(supportsWeight('inter', 450), true)
  assert.equal(parseFontPrefs('{"family":"bevietnam","weight":450}').weight, 400)
})

test('pickSize: listed sizes, below the list the smallest, otherwise the fallback', () => {
  assert.equal(pickSize('14', APP_FONT_SIZES, 13), 14)
  assert.equal(pickSize('10', APP_FONT_SIZES, 13), 13)
  assert.equal(pickSize(12, CHAT_FONT_SIZES, 14), 13)
  assert.equal(pickSize(99, CHAT_FONT_SIZES, 14), 14)
  assert.equal(pickSize('x', CHAT_FONT_SIZES, 14), 14)
})

test('automatic smoothing is crisp on a 1x screen and thin on Retina', () => {
  assert.equal(resolveSmoothing('auto', 1), 'crisp')
  assert.equal(resolveSmoothing('auto', 2), 'thin')
  assert.equal(resolveSmoothing('thin', 1), 'thin')
  assert.equal(resolveSmoothing('crisp', 2), 'crisp')
})

test('every font, code font and smoothing choice has a label in en and vi', () => {
  const cap = (s) => s.charAt(0).toUpperCase() + s.slice(1)
  for (const locale of ['en', 'vi']) {
    const general = JSON.parse(readFileSync(new URL(`../../i18n/locales/${locale}/settings.json`, import.meta.url), 'utf8')).general
    for (const f of FONT_FAMILIES) {
      assert.ok(general[`font${cap(f)}`], `${locale}: ${f}`)
      assert.ok(general[`font${cap(f)}Hint`], `${locale}: ${f} hint`)
    }
    for (const m of MONO_FONTS) assert.ok(general[`mono${cap(m)}`], `${locale}: ${m}`)
    for (const s of ['Auto', 'Crisp', 'Thin']) assert.ok(general[`smoothing${s}`], `${locale}: ${s}`)
  }
})

test('every bundled font file is declared and every declared file exists', () => {
  const css = readFileSync(new URL('../../assets/fonts.css', import.meta.url), 'utf8')
  const files = [...css.matchAll(/url\("\.\/fonts\/([^"]+)"\)/g)].map((m) => m[1])
  assert.ok(files.length > 0)
  for (const f of files) assert.ok(readFileSync(new URL(`../../assets/fonts/${f}`, import.meta.url)).length > 0, f)
  for (const family of ['Be Vietnam Pro', 'IBM Plex Sans', 'Noto Sans', 'JetBrains Mono', 'IBM Plex Mono']) {
    assert.ok(css.includes(`font-family: "${family}";`), family)
  }
})
