// Source checks for V3-R30 (V3-49 AC-1 / AC-9, V3-45 AC-6).
// Run: node --test src/lib/__tests__/v3r30Layout.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(`../../components/${p}`, import.meta.url), 'utf8')

test('V3-49 AC-1: tab padding is tight enough for 5 tabs in a 349px column', () => {
  const src = read('library/LibraryKindTabs.vue')
  assert.match(src, /border-b-2 px-2 text-xs/)
})

test('V3-49 AC-9: an empty project shows libraryFilesEmpty under the tabs', () => {
  const src = read('projects/ProjectLibrary.vue')
  assert.match(src, /files\.length === 0 \? t\('projectDetail\.libraryFilesEmpty'\)/)
  assert.match(src, /projectDetail\.libraryKindEmpty/)
})

test('V3-45 AC-6: path tail shrinks last and truncates', () => {
  const src = read('projects/ProjectList.vue')
  assert.match(src, /min-w-0 shrink-\[100\] truncate">\{\{ p\.head/)
  assert.match(src, /shrink-\[1000\] truncate">\{\{ p\.middle/)
  assert.match(src, /shrink-\[1\] truncate">\{\{ p\.tail/)
})
