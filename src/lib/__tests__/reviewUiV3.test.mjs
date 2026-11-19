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

test('UR-11: the Contents button leaves the toolbar; closed it sits above the markdown, open its close button is in the column title', () => {
  const viewer = read('../../components/FileViewer.vue')
  assert.doesNotMatch(viewer, /<OutlineToggle v-if="hasOutline" \/>/)
  assert.match(viewer, /<OutlineToggle v-if="hasOutline && !uiLayout\.fileOutlineOpen" \/>/)
  assert.match(read('../../components/files/OutlineToggle.vue'), /ui\.setFileOutlineOpen\(true\)/)
  const outline = read('../../components/files/MarkdownOutline.vue')
  assert.match(outline, /ui\.setFileOutlineOpen\(false\)/)
  assert.match(outline, /t\('files\.outline\.hide'\)/)
})

test('UR-26: a dragged board card is drawn under the pointer, offset by where it was grabbed', () => {
  const drag = read('../../components/board/useBoardDrag.ts')
  assert.match(drag, /grabX: rect \? e\.clientX - rect\.left : 0/)
  assert.match(drag, /drag\.value = \{ run, x: e\.clientX, y: e\.clientY, over: null, grabX, grabY, width \}/)
  const view = read('../../views/BoardView.vue')
  assert.match(view, /translate: `\$\{drag\.x - drag\.grabX\}px \$\{drag\.y - drag\.grabY\}px`/)
  assert.match(view, /class="pointer-events-none fixed/)
})

test('UR-20: the composer has no expand button; the height is still set by dragging the top edge', () => {
  const composer = read('../../components/sessions/RunComposer.vue')
  const hook = read('../../components/sessions/useRunComposer.ts')
  for (const src of [composer, hook]) {
    assert.doesNotMatch(src, /composerExpanded|toggleComposerExpand/)
  }
  assert.doesNotMatch(composer, /Maximize2|Minimize2/)
  assert.match(composer, /startComposerDrag/)
  assert.match(hook, /function startComposerDrag/)
  for (const lang of ['en', 'vi']) {
    const run = json(`../../i18n/locales/${lang}/run.json`)
    assert.equal(run.expandComposer, undefined, lang)
    assert.equal(run.collapseComposer, undefined, lang)
  }
})

test('UR-34: the three shared components show their explanation in an InfoTip, not a paragraph', () => {
  const row = read('../../components/settings/SettingsRow.vue')
  assert.match(row, /<InfoTip v-if="tip" :text="tip" \/>/)
  assert.doesNotMatch(row, /<p v-if="description"/)
  const head = read('../../components/settings/SettingsSectionHead.vue')
  assert.match(head, /<InfoTip :text="intro" \/>/)
  assert.doesNotMatch(head, /<p /)
  const section = read('../../components/listDetail/DetailSection.vue')
  assert.match(section, /<InfoTip v-if="hint && !hintVisible" :text="hint" \/>/)
  assert.match(section, /<p v-if="hint && hintVisible"/)
  // The warning that must stay readable opts in to the visible line.
  assert.match(read('../../components/projects/ProjectOverview.vue'), /overview\.removeHint'\)" hint-visible>/)
})

test('UR-07: the Conversation tools sit at the right of the session tab bar, icon only, and the log has no tool row', () => {
  const log = read('../../components/sessions/RunLogPane.vue')
  assert.doesNotMatch(log, /MentionedFiles|run\.fork\.from|toggleContentPanel/)
  const bar = read('../../components/sessions/SessionTabBar.vue')
  assert.match(bar, /<SessionTabTools v-if="group !== 'right'" \/>/)
  assert.ok(bar.indexOf('<SessionTabTools') < bar.indexOf('class="split-btn"'))
  assert.doesNotMatch(bar, /splitView\.split'|splitView\.unsplit'|wideBar/)
  const tools = read('../../components/sessions/SessionTabTools.vue')
  for (const need of ['run.fork.fromTitle', 'streamLog.activity.expandAll', 'run.toggleContentPanel', '<MentionedFiles']) assert.ok(tools.includes(need), need)
  const mentioned = read('../../components/MentionedFiles.vue')
  assert.match(mentioned, /hover:bg-accent/)
  assert.doesNotMatch(mentioned, /hover:bg-card/)
  for (const lang of ['en', 'vi']) {
    const sv = json(`../../i18n/locales/${lang}/run.json`).splitView
    assert.equal('split' in sv || 'unsplit' in sv, false, lang)
  }
})

test('UR-03: the shared flags card drops the long note and opens Settings › Flag from a header icon', () => {
  const src = read('../../components/projects/ProjectFlags.vue')
  const template = src.slice(src.indexOf('<template>'))
  const start = template.indexOf('sharedTitle')
  const header = template.slice(start, template.indexOf('</template>', start))
  // The settings button sits in the card header, shows only when Settings › Flag exists and opens it.
  assert.match(header, /<span v-if="canManage" class="ml-auto[^"]*">\s*<Button variant="ghost" size="icon-sm"[^>]*@click="openManage">\s*<Settings /)
  assert.match(header, /:title="t\('sessionFlags\.project\.sharedSettingsTip'\)"/)
  assert.doesNotMatch(src, /sharedNote|settingsLink/)
  for (const lang of ['en', 'vi']) {
    const project = json(`../../i18n/locales/${lang}/sessionFlags.json`).project
    assert.ok(project.sharedSettingsTip, lang)
    assert.equal(project.sharedNote, undefined, lang)
    assert.equal(project.settingsLink, undefined, lang)
  }
})

test('UR-06: header order is title, bookmarks, find, flag chip, pop-out; ⋯ only in a session window', () => {
  const src = read('../../components/sessions/RunHeader.vue')
  const t = src.slice(src.indexOf('<template>'))
  const at = (s) => t.indexOf(s)
  assert.ok(at('<SessionTitle') < at('<SessionBookmarksMenu') && at('<SessionBookmarksMenu') < at('run.findInSession'))
  assert.ok(at('run.findInSession') < at('<SessionFlagChip') && at('<SessionFlagChip') < at('run.dockToMain'))
  assert.ok(at('run.dockToMain') < at('<DropdownMenu'))
  // The ⋯ menu only renders when the window has blocks (the main window has none).
  assert.match(t, /<DropdownMenu v-if="menuBlocks\.length"/)
  assert.doesNotMatch(src, /openProjectIn|viewProject|projectSettings|handleOpenIn/)
  for (const lang of ['en', 'vi']) {
    const run = json(`../../i18n/locales/${lang}/run.json`)
    for (const k of ['openProjectIn', 'viewProject', 'projectSettings', 'projectSettingsTitle', 'folder', 'terminal']) {
      assert.equal(run[k], undefined, `${lang} ${k}`)
    }
  }
})
