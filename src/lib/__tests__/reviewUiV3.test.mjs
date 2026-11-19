// UI review v3 fixes (UR-xx): source-level checks of template rules that have
// no pure function to test, in the style of v3r31.test.mjs.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')
const json = (p) => JSON.parse(read(p))

test('UR-02: the flag menu starts with "No flag" and leads to the project\'s flags', () => {
  const src = read('../../components/sessions/FlagMenuItems.vue')
  const template = src.slice(src.indexOf('<template>'))
  // "No flag" is the first row and is ticked when the Session has no flag.
  assert.ok(template.indexOf("sessionFlags.menu.none") < template.indexOf('flagLabel(flag, t)'))
  assert.match(template, /<Check v-if="!flagId"/)
  // Groups are split by separators, with no uppercase group headings and no "Remove flag" row.
  assert.doesNotMatch(template, /uppercase/)
  assert.doesNotMatch(src, /sessionFlags\.menu\.remove/)
  // "Manage flags" opens the Session's project, not Settings › Flag.
  assert.match(template, /openProjectFlags\(projectId\)/)
  assert.doesNotMatch(src, /openManage/)
})

test('UR-02: flag menu labels carry no trailing ellipsis', () => {
  for (const lang of ['en', 'vi']) {
    const menu = json(`../../i18n/locales/${lang}/sessionFlags.json`).menu
    assert.ok(menu.none, lang)
    assert.equal(menu.remove, undefined, lang)
    assert.doesNotMatch(menu.manage, /…$/, lang)
  }
})

test('UR-05: focus mode is gone from the store, the app menu and the session header', () => {
  for (const p of ['../../stores/uiLayout.ts', '../../App.vue', '../../lib/appMenu.ts', '../../components/sessions/RunHeader.vue', '../../views/RunView.vue']) {
    assert.doesNotMatch(read(p), /focusMode|toggleFocus\b/, p)
  }
  for (const lang of ['en', 'vi']) {
    const menu = json(`../../i18n/locales/${lang}/menu.json`)
    assert.equal(menu.enterFocusMode, undefined, lang)
    assert.equal(json(`../../i18n/locales/${lang}/run.json`).focusMode, undefined, lang)
  }
})

test('UR-08: scroll column is Top · prompt navigator · Latest, sized by its counter, beside a log gutter', () => {
  const src = read('../../components/sessions/RunLogPane.vue')
  const top = src.indexOf('@click="scrollToFirstTurn"')
  const nav = src.indexOf(':aria-label="t(\'run.promptNav\')"')
  const latest = src.indexOf('@click="jumpToLatest"')
  assert.ok(top > 0 && top < nav && nav < latest)
  // A fixed w-8 cut "21/22" off; the pill now grows with the counter.
  assert.doesNotMatch(src, /flex w-8 flex-col items-center rounded-full/)
  // Both log scrollers leave room for the floating column.
  assert.equal(src.match(/overflow-auto pl-6 pr-14 py-5/g)?.length, 2)
})

test('UR-12: the Quote note popover shows its keys in the placeholder, not a row of their own', () => {
  const src = read('../../components/QuoteNotePopover.vue')
  assert.match(src, /:placeholder="t\('selection\.quoteNote\.notePlaceholderKeys'\)"/)
  assert.doesNotMatch(src, /keysSave|keysNewline/)
  for (const lang of ['en', 'vi']) {
    // Three lines: the note prompt, ↵ save, ⇧↵ new line.
    assert.equal(json(`../../i18n/locales/${lang}/selection.json`).quoteNote.notePlaceholderKeys.split('\n').length, 3, lang)
  }
})

test('UR-26: a dragged board card is drawn under the pointer, offset by where it was grabbed', () => {
  const drag = read('../../components/board/useBoardDrag.ts')
  assert.match(drag, /grabX: rect \? e\.clientX - rect\.left : 0/)
  assert.match(drag, /drag\.value = \{ run, x: e\.clientX, y: e\.clientY, over: null, grabX, grabY, width \}/)
  const view = read('../../views/BoardView.vue')
  assert.match(view, /translate: `\$\{drag\.x - drag\.grabX\}px \$\{drag\.y - drag\.grabY\}px`/)
  assert.match(view, /class="pointer-events-none fixed/)
})
