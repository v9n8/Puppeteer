// UI review v3, design round DS-4 (12 flag / project colours, 6 × 2 picker): source-level
// checks of template rules that have no pure function to test, in the style of reviewUiV3ds3.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')

test('UR-64: ColorSwatchGrid is a 6-column grid of 28px buttons around 20px dots, picked = outline + check', () => {
  const src = read('../../components/ColorSwatchGrid.vue')
  assert.match(src, /class="grid w-fit grid-cols-6 gap-1"/)
  assert.match(src, /v-for="c in PROJECT_COLORS"/)
  assert.match(src, /relative flex h-7 w-7 items-center justify-center rounded-full[^"]*hover:bg-accent[^"]*focus-visible:ring-2 focus-visible:ring-ring/)
  assert.match(src, /class="flex h-5 w-5 items-center justify-center rounded-full"/)
  assert.match(src, /selected === c && 'outline-2 outline-offset-2 outline-foreground'/)
  assert.match(src, /<Check v-if="selected === c"/)
  assert.match(src, /:aria-label="label\(c\)"/)
  assert.match(src, /:aria-pressed="selected === c"/)
})

test('UR-64: the flag dialog, the tab menu and the flag colour popover all use the grid', () => {
  for (const file of ['flags/FlagEditorModal.vue', 'WorkspaceTabs.vue', 'flags/FlagColorPopover.vue']) {
    const src = read(`../../components/${file}`)
    assert.match(src, /<ColorSwatchGrid\b/, file)
    assert.doesNotMatch(src, /v-for="\w+ in PROJECT_COLORS"/, `${file} still draws its own swatches`)
  }
  // The tab menu keeps "clear colour" on the right, top-aligned with the grid.
  const tabs = read('../../components/WorkspaceTabs.vue')
  assert.match(tabs, /<div class="flex items-start gap-1">\s*<ColorSwatchGrid :selected="projectColor\(menuProject\.color\)" @select="setColor" \/>\s*<button/)
  // The popover still names the other flags on a colour.
  const pop = read('../../components/flags/FlagColorPopover.vue')
  assert.match(pop, /:hint="sharedWith"/)
  assert.match(pop, /sharedWith\(shown\)/)
})
