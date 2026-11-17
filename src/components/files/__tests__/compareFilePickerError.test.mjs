// F-R3-7 (V3-25): a failing file listing must show an error line, not an empty list.
// Run: node --test src/components/files/__tests__/compareFilePickerError.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const src = readFileSync(new URL('../CompareFilePicker.vue', import.meta.url), 'utf8')
const locale = (l) => JSON.parse(readFileSync(new URL(`../../../i18n/locales/${l}/files.json`, import.meta.url), 'utf8'))

test('the picker keeps the listing error and renders it', () => {
  assert.match(src, /const error = ref</)
  assert.match(src, /error\.value = /)
  assert.match(src, /files\.compare\.listFailed/)
  assert.doesNotMatch(src, /catch \{\s*files\.value = \[\]\s*\}/)
})

test('the error line exists in en and vi', () => {
  for (const l of ['en', 'vi']) assert.ok(locale(l).compare.listFailed?.includes('{error}'), l)
})
