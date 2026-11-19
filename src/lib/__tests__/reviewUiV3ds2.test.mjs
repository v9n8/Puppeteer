// UI review v3, design round DS-2 (Board column filter): source-level checks of template
// rules that have no pure function to test, in the style of reviewUiV3ds3.test.mjs.
// The filter rules themselves are tested in boardFilter.test.mjs.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, statSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')

test('UR-61: BoardView stays small; the filter and its empty state are wired in', () => {
  assert.ok(statSync(new URL('../../views/BoardView.vue', import.meta.url)).size <= 20 * 1024, 'BoardView.vue over 20 KB')
  const view = read('../../views/BoardView.vue')
  assert.match(view, /<BoardFilterPopover\s+v-model:open="filterOpen"/)
  assert.match(view, /:badge="isDefault \? null : columns\.length"/)
  assert.match(view, /v-if="board\.data && columns\.length"/)
  assert.match(view, /data-board-empty/)
  assert.match(view, /t\('board\.empty\.open'\)/)
})

test('UR-61: the popover keeps ticks as a draft and asks with three buttons before dropping them', () => {
  const pop = read('../../components/board/BoardFilterPopover.vue')
  assert.match(pop, /class="absolute left-0 top-full z-50 mt-1\.5 w-\[280px\]/)
  assert.match(pop, /max-h-\[420px\] overflow-y-auto/)
  assert.match(pop, /inline-flex h-6 min-w-0 items-center gap-1\.5 rounded-full bg-muted px-2\.5 text-2xs text-muted-foreground/)
  assert.match(pop, /class="pl-5"/)
  assert.match(pop, /rounded-full bg-primary\/12 px-1\.5 text-2xs font-medium text-primary/)
  assert.match(pop, /altLabel: t\('board\.filter\.discard'\)/)
  assert.match(pop, /:disabled="!dirty" @click="revert"/)
  assert.match(pop, /:disabled="!dirty" @click="apply"/)
  // The confirm box owns Esc while it is up.
  assert.match(pop, /if \(e\.key !== 'Escape' \|\| confirming\.value\) return/)
})
