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
