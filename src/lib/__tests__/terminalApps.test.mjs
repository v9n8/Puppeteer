// Unit tests for lib/terminalApps.ts (terminal choices per OS in Settings).
// Run: node --test src/lib/__tests__/terminalApps.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { TERMINAL_APPS, desktopOs } from '../terminalApps.ts'

test('desktopOs: reads the webview user agent of each OS', () => {
  assert.equal(desktopOs('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15'), 'macos')
  assert.equal(desktopOs('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/130.0'), 'windows')
  assert.equal(desktopOs('Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15'), 'linux')
})

test('TERMINAL_APPS: macOS keeps Terminal.app as the default, iTerm only there', () => {
  assert.deepEqual(TERMINAL_APPS.macos.map((a) => a.value), ['terminal', 'iterm'])
  for (const os of ['windows', 'linux']) {
    assert.ok(!TERMINAL_APPS[os].some((a) => a.value === 'terminal' || a.value === 'iterm'), os)
  }
})

test('TERMINAL_APPS: same values and defaults as the Rust side', () => {
  const rust = readFileSync(new URL('../../../src-tauri/src/commands/projects.rs', import.meta.url), 'utf8')
  const body = rust.slice(rust.indexOf('fn terminal_apps('), rust.indexOf('pub fn resolve_terminal_app('))
  const listOf = (arm) => [...body.match(new RegExp(`${arm} => &\\[([^\\]]*)\\]`))[1].matchAll(/"([^"]+)"/g)].map((m) => m[1])
  assert.deepEqual(TERMINAL_APPS.macos.map((a) => a.value), listOf('"macos"'))
  assert.deepEqual(TERMINAL_APPS.windows.map((a) => a.value), listOf('"windows"'))
  assert.deepEqual(TERMINAL_APPS.linux.map((a) => a.value), listOf('_'))
})

test('TERMINAL_APPS: every label has an en and vi message', () => {
  for (const locale of ['en', 'vi']) {
    const general = JSON.parse(readFileSync(new URL(`../../i18n/locales/${locale}/settings.json`, import.meta.url), 'utf8')).general
    for (const app of Object.values(TERMINAL_APPS).flat()) {
      assert.equal(typeof general[app.labelKey], 'string', `${locale}: ${app.labelKey}`)
    }
  }
})
