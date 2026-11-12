// Unit tests for the permission mode menu (UX-70): the modes Claude's own
// Permissions menu lists, each named alone with its explanation as a tooltip.
// Run: node --test src/lib/__tests__/permissionModes.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { PERMISSION_MODE_CHOICES, permissionModeOptions, resolvePermissionMode } from '../engineOptions.ts'

const settings = (lang) =>
  JSON.parse(readFileSync(new URL(`../../i18n/locales/${lang}/settings.json`, import.meta.url), 'utf8'))

test('menu: Claude\'s order, no "from settings" row', () => {
  const values = permissionModeOptions((key) => key).map((o) => o.value)
  assert.deepEqual(values, ['auto', 'default', 'acceptEdits', 'plan', 'bypassPermissions'])
})

test('menu: the label is the mode\'s name and the explanation is a tooltip, not a second line', () => {
  const [auto] = permissionModeOptions((key) => key)
  assert.deepEqual(auto, {
    value: 'auto',
    label: 'settings.ai.permModes.auto.label',
    tooltip: 'settings.ai.permModes.auto.hint',
  })
})

test('menu: every mode has a label and a tooltip in en and vi', () => {
  for (const lang of ['en', 'vi']) {
    const modes = settings(lang).ai.permModes
    for (const mode of PERMISSION_MODE_CHOICES) {
      assert.ok(modes[mode]?.label?.trim(), `${lang}: ${mode} label`)
      assert.ok(modes[mode]?.hint?.trim(), `${lang}: ${mode} hint`)
    }
  }
})

test('menu: every listed mode is one a launch accepts', () => {
  for (const mode of PERMISSION_MODE_CHOICES) assert.equal(resolvePermissionMode(mode, ''), mode)
})
