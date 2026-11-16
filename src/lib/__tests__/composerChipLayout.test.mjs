// Unit tests for lib/composerChipLayout.ts (when Effort / Response style fold into Run options).
// Run: node --test src/lib/__tests__/composerChipLayout.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { hideOptionalChips } from '../composerChipLayout.ts'

test('under 700px the chips are always hidden', () => {
  assert.equal(hideOptionalChips(699, { hasRunActions: false }), true)
  assert.equal(hideOptionalChips(699, { hasRunActions: true }), true)
  assert.equal(hideOptionalChips(0, { hasRunActions: false }), true)
})

test('700 to 999px hides them only while run actions share the row', () => {
  assert.equal(hideOptionalChips(700, { hasRunActions: false }), false)
  assert.equal(hideOptionalChips(700, { hasRunActions: true }), true)
  assert.equal(hideOptionalChips(999, { hasRunActions: false }), false)
  assert.equal(hideOptionalChips(999, { hasRunActions: true }), true)
})

test('from 1000px the chips always show', () => {
  assert.equal(hideOptionalChips(1000, { hasRunActions: true }), false)
  assert.equal(hideOptionalChips(1000, { hasRunActions: false }), false)
  assert.equal(hideOptionalChips(1600, { hasRunActions: true }), false)
})

test('the Effort popover lists levels only: the empty "Default" row stays filtered out', async () => {
  const { readFileSync } = await import('node:fs')
  const src = readFileSync(new URL('../../components/sessions/useComposerChips.ts', import.meta.url), 'utf8')
  assert.match(src, /defaultEffortChoices\([^)]*\)\s*\.filter\(\(c\) => c\.value\)/)
})
