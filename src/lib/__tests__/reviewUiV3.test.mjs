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

test('UR-09: the main window shows a short "Quote", own windows name the target; the tooltip always does', () => {
  const src = read('../../components/SelectionToolbar.vue')
  assert.match(src, /ownWindow = IS_POPOUT_WINDOW \|\| IS_SESSION_WINDOW/)
  assert.match(src, /ownWindow \? t\('selection\.quote', .*: t\('selection\.quoteShort'\)/)
  assert.match(src, /t\('selection\.quoteTitle', \{ target/)
  for (const l of ['en', 'vi']) {
    const s = json(`../../i18n/locales/${l}/selection.json`)
    assert.ok(s.quoteShort && s.quote.includes('{target}') && s.quoteTitle.includes('{target}'))
  }
})

test('UR-16: a thinking block shows its first line until opened, stays open while streamed, opens on a ⌘F hit', async () => {
  const { thinkingFindHits, thinkingOpen, thinkingPreview } = await import('../thinkingFold.ts')
  assert.equal(thinkingPreview('\n  First idea\nsecond'), 'First idea')
  // Streaming opens it; once done it folds itself unless the reader chose.
  assert.equal(thinkingOpen({ streaming: true }), true)
  assert.equal(thinkingOpen({ streaming: false }), false)
  assert.equal(thinkingOpen({ streaming: true, choice: false }), false)
  assert.equal(thinkingOpen({ choice: true }), true)
  assert.equal(thinkingOpen({ findHit: true }), true)
  const entries = [{ kind: 'user', text: 'Needle' }, { kind: 'thinking', text: 'a needle here' }, { kind: 'thinking', text: 'none' }]
  assert.deepEqual([...thinkingFindHits(entries, 'NEEDLE')], [1])
  assert.deepEqual([...thinkingFindHits(entries, 'NEEDLE', true)], [])
  assert.equal(thinkingFindHits(entries, '').size, 0)
  const block = read('../../components/sessions/ThinkingBlock.vue')
  assert.match(block, /:aria-expanded="open"/)
  assert.match(block, /<div v-if="open" class="[^"]*\[overflow-wrap:anywhere\]/)
  const log = read('../../components/StreamLog.vue')
  assert.match(log, /<ThinkingBlock v-else-if="entry\.kind === 'thinking'" [^>]*:find-hit="thinkingHits\.has\(i\)"/)
  // The memoised entry re-renders when a search hits its thinking.
  assert.match(log, /v-memo="\[entry, .*, thinkingHits\.has\(i\)\]"/)
})

test('UR-16: long text in the log wraps instead of scrolling the conversation sideways', () => {
  const log = read('../../components/StreamLog.vue')
  // No avatar beside the user's bubble; the bubble and its text break anywhere.
  assert.doesNotMatch(log, /streamLog\.you|<User\b/)
  assert.match(log, /class="min-w-0 max-w-\[82%\] [^"]*\[overflow-wrap:anywhere\]"/)
  assert.match(log, /class="min-w-0 whitespace-pre-wrap \[overflow-wrap:anywhere\]"><template/)
  // The @path chip fits the bubble, truncates, and hovers apart from the bubble colour.
  assert.match(log, /inline-flex max-w-full [^"]*hover:bg-primary\/10 hover:border-primary\/40/)
  assert.match(log, /<span class="min-w-0 truncate">\{\{ seg\.text \}\}<\/span>/)
  // stderr, AskUserQuestion, TodoWrite and the fullscreen question.
  assert.match(log, /<span class="min-w-0 whitespace-pre-wrap break-all">\{\{ entry\.text \}\}<\/span>\n {6}<\/div>\n\n {6}<!-- the turn went on/)
  assert.match(log, /text-foreground\/85 \[overflow-wrap:anywhere\]">\{\{ q\.question \}\}/)
  assert.match(log, /<div class="min-w-0 \[overflow-wrap:anywhere\]"><span class="font-medium text-foreground\/80">\{\{ opt\.label \}\}/)
  assert.match(log, /<span class="min-w-0 \[overflow-wrap:anywhere\]" :class="\{\n\s*'line-through/)
  assert.match(log, /v-else-if="fullscreen\.plain" class="min-w-0 [^"]*\[overflow-wrap:anywhere\]"/)
  // A local command's output folds to 3 lines and opens on a click; its args chip truncates.
  const cmd = read('../../components/sessions/LocalCommandMark.vue')
  assert.match(cmd, /whitespace-pre-wrap break-all/)
  assert.match(cmd, /:class="\{ 'line-clamp-3': !expanded \}"/)
  assert.match(cmd, /@click="expanded = !expanded"/)
  assert.match(cmd, /class="min-w-0 truncate font-mono[^"]*" :title="args"/)
  for (const lang of ['en', 'vi']) {
    const s = json(`../../i18n/locales/${lang}/streamLog.json`)
    assert.equal(s.you, undefined, lang)
    for (const k of ['showThinking', 'hideThinking', 'showOutput', 'hideOutput']) assert.ok(s[k], `${lang}.${k}`)
  }
})

test('UR-30: the Library kind picker is always a select with counts, in both places it is used', () => {
  const src = read('../../components/library/LibraryKindTabs.vue')
  assert.match(src, /<AppSelect/)
  assert.doesNotMatch(src, /role="tab"|onKeydown|compact/)
  assert.match(src, /\(\$\{props\.counts\[k\] \?\? 0\}\)/)
  for (const p of ['../../views/LibraryView.vue', '../../components/projects/ProjectLibrary.vue']) {
    assert.match(read(p), /<LibraryKindTabs [^>]*\/>/)
    assert.doesNotMatch(read(p), /<LibraryKindTabs [^>]*:compact/)
  }
})

test('UR-31: the list sidebar puts "New" beside the search box, and on the controls row only when there is one', () => {
  const src = read('../../components/listDetail/ListDetailSidebar.vue')
  const template = src.slice(src.indexOf('<template>'))
  const search = template.indexOf('<Input v-model="search"')
  const early = template.indexOf('v-if="addable && !$slots.controls" name="add"')
  const controlsRow = template.indexOf('v-if="$slots.controls"')
  const late = template.indexOf('v-if="addable" name="add"')
  assert.ok(search < early && early < controlsRow && controlsRow < late)
  // The controls row no longer exists just to hold a lone "New" button.
  assert.doesNotMatch(template, /v-if="addable \|\| \$slots\.controls"/)
  assert.match(template, /\[&>\*\]:w-full/)
})

test('UR-32: list pane width is clamped, remembered per screen, and every list screen can drag it', async () => {
  const { clampPaneWidth, parsePaneWidth, LIST_PANE_MIN, LIST_PANE_MAX } = await import('../listPaneWidth.ts')
  assert.equal(clampPaneWidth(10), LIST_PANE_MIN)
  assert.equal(clampPaneWidth(9999), LIST_PANE_MAX)
  assert.equal(clampPaneWidth(300.4), 300)
  assert.equal(parsePaneWidth(null, 280), 280)
  assert.equal(parsePaneWidth('abc', 280), 280)
  assert.equal(parsePaneWidth('350', 280), 350)
  assert.equal(parsePaneWidth('50', 280), LIST_PANE_MIN)
  // The sidebar draws the handle and stores under its collapse key; Library draws its own around the kind select.
  const sidebar = read('../../components/listDetail/ListDetailSidebar.vue')
  assert.match(sidebar, /useListPaneWidth\(props\.collapseKey/)
  assert.match(sidebar, /<ListPaneResizer\s+v-if="resizable"/)
  assert.doesNotMatch(sidebar, /w-\[280px\]/)
  const library = read('../../views/LibraryView.vue')
  assert.match(library, /<ListPaneResizer/)
  assert.doesNotMatch(library, /w-\[240px\]|'w-56'/)
  // Tags go on the last line of a row, after the description.
  const row = library.slice(library.indexOf('#row="{ row, selected }"'))
  assert.ok(row.indexOf('row.item.description') < row.indexOf('library.engine.'))
  const mcp = read('../../components/settings/McpSettings.vue')
  assert.ok(mcp.indexOf('row.line || ') < mcp.indexOf("t('mcp.list.claudeOnly')"))
})
