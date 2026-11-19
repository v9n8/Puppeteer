// UI review v3 fixes (UR-xx): source-level checks of template rules that have
// no pure function to test, in the style of v3r31.test.mjs.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { markDefault, projectDefaultSelect } from '../defaultMark.ts'
import { composerModelChoices, shownModel } from '../composerChips.ts'
import { FOLLOW_HOST, overrideForPick } from '../controllerAppearance.ts'

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

test('UR-04: a new migration resets the flags to the five work-status defaults, matched in code and i18n', () => {
  const keys = ['backlog', 'todo', 'in_progress', 'in_review', 'done']
  const sql = read('../../../src-tauri/migrations/0025_default_flags_v2.sql')
  // Every flag goes (0023's trigger clears the sessions and hidden marks), then the five are seeded in order.
  assert.match(sql, /^DELETE FROM session_flags;$/m)
  const seeded = [...sql.matchAll(/\('(\w+)', NULL, '[^']+', '(\w+)', (\d), '\1',/g)].map((m) => [m[1], m[2], Number(m[3])])
  assert.deepEqual(seeded, [['backlog', 'neutral', 0], ['todo', 'primary', 1], ['in_progress', 'info', 2], ['in_review', 'warning', 3], ['done', 'success', 4]])
  const rust = read('../../../src-tauri/src/commands/session_flags.rs')
  for (const [key, color] of seeded) assert.match(rust, new RegExp(`DefaultFlag \\{ key: "${key}", name: "[^"]+", color: "${color}" \\}`), key)
  assert.match(read('../sessionFlags.ts'), /DEFAULT_FLAG_KEYS = \['backlog', 'todo', 'in_progress', 'in_review', 'done'\]/)
  for (const lang of ['en', 'vi']) {
    assert.deepEqual(Object.keys(json(`../../i18n/locales/${lang}/sessionFlags.json`).builtin), keys, lang)
  }
  assert.equal(json('../../i18n/locales/en/sessionFlags.json').builtin.todo, 'To Do')
})

test('UR-19: the header block around the session title takes the free width so the rename field can grow', () => {
  const src = read('../../components/sessions/RunHeader.vue')
  assert.match(src, /<div class="flex flex-1 items-center gap-2 min-w-0">\s*<SessionTitle \/>/)
  assert.match(read('../../components/sessions/SessionTitle.vue'), /<form v-if="editing" class="flex min-w-0 flex-1 /)
})

test('UR-10: detach / merge-back / dock-back buttons are icon-only with a tooltip and no label span', () => {
  for (const f of ['app/DetachButton.vue', 'app/MergeBackButton.vue', 'sessions/FileTabView.vue']) {
    const src = read(`../../components/${f}`)
    assert.doesNotMatch(src, /max-\[901px\]:hidden/, f)
    assert.match(src, /size="icon-sm"/, f)
    assert.match(src, /:aria-label=/, f)
  }
  const win = read('../../views/FileViewerWindow.vue')
  assert.doesNotMatch(win, /files\.viewer\.dockBack['"]/)
  assert.match(win, /:aria-label="t\('files\.viewer\.dockBackTip'\)"/)
  assert.doesNotMatch(read('../../components/sessions/FileTabView.vue'), /nav\.detachWindow['"]/)
  for (const lang of ['en', 'vi']) {
    assert.equal(json(`../../i18n/locales/${lang}/nav.json`).detachWindow, undefined, lang)
    assert.equal(json(`../../i18n/locales/${lang}/files.json`).viewer.dockBack, undefined, lang)
    assert.ok(json(`../../i18n/locales/${lang}/files.json`).viewer.dockBackTip, lang)
  }
})

test('UR-22: a default-value select shows a real value and stars the default, never a "follow" row', () => {
  const modes = [{ value: 'auto', label: 'Auto' }, { value: 'plan', label: 'Plan' }]
  // Nothing saved: the global value is shown and starred.
  const unsaved = projectDefaultSelect(modes, null, 'auto')
  assert.equal(unsaved.value, 'auto')
  assert.deepEqual(unsaved.options.map((o) => [o.value, !!o.isDefault]), [['auto', true], ['plan', false]])
  // Saved: the project's value is shown; the star stays on the global one.
  assert.equal(projectDefaultSelect(modes, 'plan', 'auto').value, 'plan')
  // An "engine decides" row ('') is listed only while it is the global value.
  const levels = [{ value: '', label: 'Engine' }, { value: 'high', label: 'High' }]
  assert.deepEqual(projectDefaultSelect(levels, null, 'high').options.map((o) => o.value), ['high'])
  assert.deepEqual(projectDefaultSelect(levels, 'high', '').options.map((o) => [o.value, !!o.isDefault]), [['', true], ['high', false]])
  // Header rows are never starred.
  assert.equal(markDefault([{ value: 'x', label: 'X', header: true }, { value: 'x', label: 'X' }], 'x')[1].isDefault, true)
})

test('UR-22: the composer model list shows the model in force once and stars the project default', () => {
  const opts = [
    { value: '', label: 'Default' },
    { value: '__group:claude', label: 'Claude', header: true },
    { value: 'opus', label: 'opus' },
    { value: 'sonnet', label: 'sonnet' },
  ]
  const listed = composerModelChoices(opts, 'opus', 'sonnet')
  assert.deepEqual(listed.map((o) => o.value), ['__group:claude', 'opus', 'sonnet'])
  assert.deepEqual(listed.filter((o) => o.isDefault).map((o) => o.value), ['sonnet'])
  assert.equal(shownModel('', 'opus', listed), 'opus')
  assert.equal(shownModel('sonnet', 'opus', listed), 'sonnet')
  // No model configured anywhere: the Default row stays, shown and starred.
  const none = composerModelChoices(opts, '', '')
  assert.equal(none[0].value, '')
  assert.equal(none[0].isDefault, true)
  assert.equal(shownModel('', '', none), '')
})

test('UR-22: AppSelect stars isDefault rows; the project page and composer chips use it', () => {
  const select = read('../../components/AppSelect.vue')
  assert.match(select, /v-if="option\.isDefault"[\s\S]*?<Star /)
  assert.match(select, /t\('common\.default'\)/)
  const project = read('../../components/ProjectSessionDefaults.vue')
  assert.doesNotMatch(project, /sessionDefaults\.follow/)
  assert.equal((project.match(/projectDefaultSelect(<\w+>)?\(/g) ?? []).length, 6)
  const chips = read('../../components/sessions/useComposerChips.ts')
  assert.equal((chips.match(/markDefault(<\w+>)?\(/g) ?? []).length, 3)
  assert.match(read('../../components/sessions/RunComposer.vue'), /:model-value="shownModel\(modelOverride, defaultModel, modelOptions\)"/)
  for (const lang of ['en', 'vi']) {
    assert.ok(json(`../../i18n/locales/${lang}/common.json`).default, lang)
    assert.equal(json(`../../i18n/locales/${lang}/projectDetail.json`).sessionDefaults.follow, undefined, lang)
    assert.equal(json(`../../i18n/locales/${lang}/run.json`).modelDefaultWith, undefined, lang)
  }
})

test('UR-01: "Clear all" leaves History and lives in the Project overview, which forgets the deleted tabs', () => {
  const rail = read('../../components/sessions/RunSessionRail.vue')
  assert.doesNotMatch(rail, /handleClearAllRuns|clearingAll|run\.clearAll/)
  assert.doesNotMatch(read('../../components/sessions/useRunHistoryList.ts'), /handleClearAllRuns|clearingAll/)
  const overview = read('../../components/projects/ProjectOverview.vue')
  assert.match(overview, /runsStore\.fetchRuns\(project\.id\)/)
  assert.match(overview, /runsStore\.deleteAllRuns\(project\.id\)/)
  assert.match(overview, /sessionTabs\.forget\(/)
  assert.match(overview, /@click="clearSessions"/)
  for (const lang of ['en', 'vi']) {
    const run = json(`../../i18n/locales/${lang}/run.json`)
    for (const k of ['clearAll', 'clearAllRunsTitle', 'clearing']) assert.equal(run[k], undefined, `${lang} run.${k}`)
    assert.ok(json(`../../i18n/locales/${lang}/projects.json`).overview.clearSessions, lang)
  }
})

test('UR-15: compact rows show only the flag tag on line 2; the time steps aside on hover; re-sort is an icon', () => {
  const rail = read('../../components/sessions/RunSessionRail.vue')
  const compact = rail.slice(rail.indexOf("v-if=\"historyDensity === 'compact'\""), rail.indexOf("v-else-if=\"historyDensity === 'comfortable'\""))
  assert.doesNotMatch(compact, /runAccountModelLine|runModelLabel|runClaudeAccountLabel/)
  assert.match(compact, /projectTileClass\(runFlag\(run\)!\.color\)/)
  assert.match(compact, /rounded-full/)
  assert.match(compact, /invisible: !runFlag\(run\)/)
  // Relative time hides while the ⋯ block shows, in compact as well as dense.
  assert.match(rail, /:class="\['group-hover:invisible group-focus-within:invisible'/)
  // Re-sort: icon button, tooltip kept, no visible label.
  assert.doesNotMatch(rail, /run\.resortSessions['"]/)
  assert.match(rail, /:aria-label="t\('run\.resortSessionsTitle'\)"/)
  for (const lang of ['en', 'vi']) assert.equal(json(`../../i18n/locales/${lang}/run.json`).resortSessions, undefined, lang)
})

test('UR-43: the History flag filter has one "Manage flags" entry, which opens the project\'s flags', () => {
  const src = read('../../components/sessions/HistoryFlagFilter.vue')
  const template = src.slice(src.indexOf('<template>'))
  assert.equal((template.match(/sessionFlags\.menu\.manage/g) ?? []).length, 1)
  assert.match(template, /openProjectFlags\(projectId\)/)
  assert.doesNotMatch(src, /openManage|canManage|filter\.projectFlags/)
  assert.match(template, /board\.viewAsBoard/)
  for (const lang of ['en', 'vi']) assert.equal(json(`../../i18n/locales/${lang}/sessionFlags.json`).filter.projectFlags, undefined, lang)
})

test('UR-18: the sidebar toggle sits in the sidebar top row, not the title bar', () => {
  const bar = read('../../components/app/AppTitlebar.vue')
  assert.doesNotMatch(bar, /toggleSidebar|PanelLeft|showSidebar/)
  const side = read('../../components/app/AppSidebar.vue')
  const tpl = side.slice(side.indexOf('<template>'))
  // First child of the sidebar column, before the nav; right-aligned in full, centred in the rail.
  assert.ok(tpl.indexOf('uiLayout.toggleSidebar()') < tpl.indexOf('<nav'))
  assert.match(tpl, /:class="rail \? 'justify-center' : 'justify-end'"/)
  assert.match(tpl, /rail \? t\('menu\.showSidebar'\) : t\('menu\.hideSidebar'\)/)
})

test('UR-14: the MCP section explains when a change applies through an InfoTip, not a paragraph', () => {
  const src = read('../../components/sessions/RunMcpSection.vue')
  const template = src.slice(src.indexOf('<template>'))
  assert.match(template, /<InfoTip :text="running \? t\('run\.mcpAppliesAfterRun'\) : t\('run\.mcpAppliesNextMessage'\)"/)
  assert.doesNotMatch(template, /<p[^>]*>\s*\{\{ running \?/)
  // The tip sits beside the title, before the server list.
  assert.ok(template.indexOf('<InfoTip') < template.indexOf('<ul'))
})

test('UR-23: segmented control labels never wrap; they truncate and keep a full-text title', () => {
  const src = read('../../components/ui/SegmentedControl.vue')
  assert.match(src, /whitespace-nowrap/)
  assert.match(src, /<span v-if="!iconOnly" class="min-w-0 truncate">\{\{ o\.label \}\}<\/span>/)
  assert.match(src, /:title="o\.title \?\? o\.label"/)
})

test('UR-21: the context chip keeps /compact as one short label and moves the hint into its title', () => {
  const src = read('../../components/sessions/SessionContextChip.vue')
  assert.match(src, /<DropdownItem[^>]*:title="t\('run\.infoBar\.context\.compactHint'\)"[^>]*sendSlashCommand\('\/compact'\)/)
  // The hint is no longer a second line inside the menu row.
  assert.doesNotMatch(src, /<span[^>]*>\{\{ t\('run\.infoBar\.context\.compactHint'\) \}\}<\/span>/)
  for (const lang of ['en', 'vi']) assert.match(read(`../../i18n/locales/${lang}/run.json`), /"compactHint": "/)
})

test('UR-38: long unbroken text wraps inside flex rows instead of overflowing', () => {
  const wraps = [
    ['../../components/DiffView.vue', /class="min-w-0 flex-1 px-2 whitespace-pre-wrap break-words \[overflow-wrap:anywhere\]"/],
    ['../../components/permission/PermissionRequestBody.vue', /min-w-0 whitespace-pre-wrap break-words \[overflow-wrap:anywhere\] px-2 text-red/],
    ['../../components/permission/PermissionRequestBody.vue', /min-w-0 whitespace-pre-wrap break-words \[overflow-wrap:anywhere\] px-2 text-emerald/],
    ['../../components/ui/ToastHost.vue', /min-w-0 flex-1 text-foreground leading-snug \[overflow-wrap:anywhere\]/],
    ['../../components/ui/ConfirmModal.vue', /self-center min-w-0 \[overflow-wrap:anywhere\]/],
    ['../../components/conductor/WorkerResultCards.vue', /mt-1 min-w-0 text-foreground\/60 \[overflow-wrap:anywhere\]/],
    ['../../components/conductor/WorkerResultCards.vue', /mt-1 min-w-0 \[overflow-wrap:anywhere\] whitespace-pre-wrap/],
    ['../../components/conductor/ConductorWorkerPanel.vue', /min-w-0 text-foreground\/80 \[overflow-wrap:anywhere\]/],
    ['../../components/FileTree.vue', /text-destructive break-all">\{\{ rootError/],
    ['../../components/projects/ProjectBoard.vue', /text-foreground break-all">\{\{ project\.github_project_board_url/],
  ]
  for (const [file, re] of wraps) assert.match(read(file), re, file)
  // Account rows: only the label truncates; the "default" tag keeps its width.
  for (const file of ['../../components/settings/ConnectionsSettings.vue', '../../components/settings/GoogleAccountsSettings.vue']) {
    const src = read(file)
    assert.match(src, /<span class="min-w-0 truncate">\{\{ (row|acc)\.label \}\}<\/span>/, file)
    assert.match(src, /shrink-0 text-2xs font-medium text-primary/, file)
    assert.doesNotMatch(src, /text-sm font-medium truncate flex items-center gap-1\.5/, file)
  }
  // The copy button gets its own gutter so it does not cover the end of the first code line.
  assert.match(read('../../assets/main.css'), /\.code-copy-wrap > pre \{\s*padding-right: 2\.6rem;/)
})

test('UR-25: the switcher row has one "Open" button that leaves the flow, and "Add project" opens the shared modal', () => {
  const sw = read('../../components/ProjectSwitcher.vue')
  assert.doesNotMatch(sw, /startSession|projectSwitcher\.newSession/)
  assert.match(sw, /variant="outline"\s+size="xs"\s+class="hidden [^"]*group-hover:flex/)
  assert.doesNotMatch(sw, /opacity-0 group-hover/)
  // The inline name-only form is gone: the switcher and the Projects page share one modal.
  assert.match(sw, /<AddProjectModal :pending="pending"/)
  assert.doesNotMatch(sw, /<form|pendingName/)
  const page = read('../../views/ProjectsView.vue')
  assert.match(page, /<AddProjectModal :pending="pending"/)
  assert.doesNotMatch(page, /<Modal/)
  const modal = read('../../components/projects/AddProjectModal.vue')
  assert.match(modal, /projects\.projectName/)
  assert.match(modal, /projects\.repositories/)
  for (const lang of ['en', 'vi']) {
    const keys = json(`../../i18n/locales/${lang}/projectSwitcher.json`)
    assert.equal('namePlaceholder' in keys, false)
    assert.equal('add' in keys, false)
  }
})

test('UR-28: the open file has no ⋯ menu; its tab and its toolbar name share one right-click menu', () => {
  const viewer = read('../../components/FileViewer.vue')
  assert.doesNotMatch(viewer, /MoreHorizontal|DropdownMenu|FileActionsMenu/)
  assert.match(viewer, /<ViewerFileName [^>]*@compare="fc\.openPicker"/)
  const name = read('../../components/files/ViewerFileName.vue')
  assert.match(name, /@contextmenu\.prevent="openMenu"/)
  assert.match(name, /<FileTabMenuItems [^>]*@compare="emit\('compare'\)"/)
  // "Copy content" is a toolbar icon now, only for a fully readable text file.
  assert.match(name, /v-if="canCopy && target"[\s\S]*actions\.copyContent\(projectPath, target\)/)
  assert.match(viewer, /:can-copy="editable"/)
  // The tab menu uses the same rows; "Compare with…" reaches the tab's viewer.
  const bar = read('../../components/sessions/SessionTabBar.vue')
  assert.match(bar, /<FileTabMenuItems [^>]*@compare="compareTab\(menu\.tab\)"/)
  assert.match(bar, /select\(tab\.id\)\s+requestCompare\(root, tab\.path\)/)
  const rows = read('../../components/files/FileTabMenuItems.vue')
  for (const key of ['copyPath', 'copyRelativePath', 'openInDefaultApp', 'openInVscode', 'openInChrome', 'mentionInChat', 'compareWith']) {
    assert.match(rows, new RegExp(`files\\.tree\\.${key}`), key)
  }
  // "Find" leaves the menu (⌘F still opens the find bar); the viewer variant is gone.
  assert.doesNotMatch(rows, /viewer\.find|copyContent/)
  assert.doesNotMatch(read('../../components/FileActionsMenu.vue'), /variant === 'viewer'|ViewerFileMenu/)
  for (const lang of ['en', 'vi']) {
    const files = json(`../../i18n/locales/${lang}/files.json`)
    assert.equal(files.viewer.find, undefined, lang)
    assert.equal(files.viewer.moreActions, undefined, lang)
  }
})

test('UR-40: picking the desktop\'s (starred) look on the phone follows the desktop; another look is the phone\'s own', () => {
  assert.equal(overrideForPick('dark', 'dark'), FOLLOW_HOST)
  assert.equal(overrideForPick('light', 'dark'), 'light')
})

test('UR-40: the other selects star the default instead of a default row or a "(default)" label', () => {
  const focus = read('../../components/FocusSettings.vue')
  assert.match(focus, /markDefault\(/)
  assert.doesNotMatch(focus, /calendar\.defaultAccount/)
  const schedules = read('../../views/SchedulesView.vue')
  assert.match(schedules, /projectDefaultSelect\(\[\{ value: 'claude'/)
  assert.doesNotMatch(schedules, /schedules\.engineDefault/)
  assert.match(read('../../components/AzureAccountsSettings.vue'), /isDefault: s\.is_default/)
  assert.match(read('../../components/settings/GeneralSettings.vue'), /markDefault\(terminalApps\.map/)
  const theme = read('../../controller/components/ThemeSheet.vue')
  assert.doesNotMatch(theme, /FOLLOW_HOST|followHost/)
  assert.equal((theme.match(/<Star/g) ?? []).length, 2)
  const composer = read('../../controller/components/SessionComposer.vue')
  assert.match(composer, /:model-value="shownAccountId"/)
  assert.match(composer, /markDefault\(opts, defaultAccountId\.value\)/)
  for (const lang of ['en', 'vi']) {
    const settings = json(`../../i18n/locales/${lang}/settings.json`)
    for (const key of ['terminalMacos', 'terminalPowershell']) assert.doesNotMatch(settings.general[key], /default|mặc định/i, `${lang} ${key}`)
    assert.equal(settings.azure.subscriptionDefault, undefined, lang)
    assert.equal(json(`../../i18n/locales/${lang}/focus.json`).settings.calendar.defaultAccount, undefined, lang)
    assert.equal(json(`../../i18n/locales/${lang}/schedules.json`).engineDefault, undefined, lang)
    const controller = json(`../../i18n/locales/${lang}/controller.json`)
    assert.equal(controller.theme.followHostValue, undefined, lang)
    assert.equal(controller.composer.claudeAccountDefaultWith, undefined, lang)
  }
})

test('UR-13: Quote block has a top handle, content-sized list, two-line cards and confirmed removal', () => {
  const src = read('../../components/sessions/ComposerQuoteBlock.vue')
  const template = src.slice(src.indexOf('<template>'))
  // The handle sits on the top edge, before the header, and dragging up grows the list.
  assert.match(template, /role="separator"[\s\S]*?class="group absolute inset-x-0 -top-1 /)
  assert.ok(template.indexOf('role="separator"') < template.indexOf('<header'))
  assert.match(src, /dragStartHeight \+ dragStartY - e\.clientY/)
  // Only maxHeight until the first drag, so two short quotes leave no empty space.
  assert.match(template, /maxHeight: `\$\{listHeight\}px`, \.\.\.\(userHeight == null \? \{\} : \{ height/)
  // Note on the second line, no pencil icon in front of it (the edit button keeps its own).
  assert.match(template, /v-if="q\.note\?\.trim\(\)"\s+class="truncate text-2xs text-muted-foreground"/)
  assert.equal((template.match(/<Pencil /g) ?? []).length, 1)
  // Removing one quote and "Remove all" both ask first; buttons hover to accent.
  assert.match(src, /@click="removeOne\(q\.id\)"/)
  assert.match(src, /async function removeAll\(\) \{[\s\S]*?await confirm\(/)
  assert.match(src, /async function removeOne\(id: string\) \{[\s\S]*?await confirm\(/)
  assert.doesNotMatch(src, /hover:bg-background/)
})

test('UR-24: the log find bar floats top-right with its hits, not a full-width strip', () => {
  const src = read('../../components/sessions/RunLogPane.vue')
  const tpl = src.slice(src.indexOf('<template>'))
  assert.match(tpl, /<div v-if="findOpen" class="absolute top-2 right-4 z-20 [^"]*w-fit/)
  assert.doesNotMatch(tpl, /v-if="findOpen" class="shrink-0 border-b/)
  // Hits sit inside the floating block, right after the bar.
  const at = (s) => tpl.indexOf(s)
  assert.ok(at('<FindBar :find="runFind"') < at('v-if="logSearchActive"'))
  assert.ok(at('v-if="logSearchActive"') < at('<!-- Log viewer'))
})

test('UR-46: duplicate buttons are gone (empty project log, PR inbox clear-all)', () => {
  const log = read('../../components/sessions/RunLogPane.vue')
  const empty = log.slice(log.indexOf('v-if="noSession"'), log.indexOf('<!-- Split container -->'))
  assert.match(empty, /run\.noSessionsYet/)
  assert.doesNotMatch(empty, /<Button|handleNewSession/)
  const bar = read('../../components/prInbox/PrInboxSearchBar.vue')
  assert.doesNotMatch(bar, /clearAll|clear-all/)
  const view = read('../../views/PrInboxView.vue')
  assert.doesNotMatch(view, /clearSearchAndFilters|@clear-all/)
  // Still one in the chip row and one in the filter popover.
  assert.match(view, /@click="clearAllFilters"/)
  assert.match(read('../../components/prInbox/PrInboxFiltersPopover.vue'), /prInbox\.filters\.clearAll/)
  for (const lang of ['en', 'vi']) {
    const pr = json(`../../i18n/locales/${lang}/prInbox.json`)
    assert.equal(pr.filters.clearSearchAndFilters, undefined)
  }
})

test('UR-27: action labels have no trailing ellipsis, menu bar and loading text keep it', () => {
  const dropped = [
    'projectSwitcher.addProject', 'run.accountMenu.manage', 'files.tree.compareWith',
    'focus.snooze.custom', 'interruptedRuns.choose', 'settings.general.fontCustom',
    'settings.backup.export.button', 'settings.backup.import.choose', 'projects.chooseFolder',
    'windows.openWindows', 'misc.remoteIndicator.pair',
  ]
  for (const lang of ['en', 'vi']) {
    for (const key of dropped) {
      const [file, ...path] = key.split('.')
      const value = path.reduce((o, k) => o?.[k], json(`../../i18n/locales/${lang}/${file}.json`))
      assert.equal(typeof value, 'string', `${lang} ${key}`)
      assert.ok(!value.endsWith('…'), `${lang} ${key} still ends with an ellipsis`)
    }
    // The menu bar / tray keep "…" (Q12); in-progress text keeps it too.
    assert.ok(Object.values(json(`../../i18n/locales/${lang}/menu.json`)).some((v) => v.endsWith('…')))
    assert.ok(json(`../../i18n/locales/${lang}/common.json`).loading.endsWith('…'))
  }
  // AppSelect's default placeholder comes from i18n, not a hardcoded string.
  const select = read('../../components/AppSelect.vue')
  assert.doesNotMatch(select, /'Select…'/)
  assert.match(select, /t\('common\.select'\)/)
})

test('UR-36: explanation text on project, MCP, library, Slack, server and note screens sits behind an info tip', () => {
  const tips = [
    ['projects/ProjectAccounts.vue', /<InfoTip\s+v-if="activeFilter !== 'all'"/],
    ['projects/ProjectBoard.vue', /<InfoTip :text="`\$\{t\('projectDetail\.boardIntroPrefix'\)\}/],
    ['projects/ProjectFlags.vue', /<InfoTip :text="t\('sessionFlags\.project\.footer'/],
    ['projects/ProjectOverview.vue', /<InfoTip :text="t\('projectDetail\.colorHint'\)"/],
    ['projects/ProjectServers.vue', /<InfoTip :text="t\('projectDetail\.toolPermsIntro'\)"/],
    ['projects/ProjectList.vue', /<InfoTip :text="t\('projects\.dotsLegend'\)"/],
    ['TodoChecklistPicker.vue', /<InfoTip :text="t\('run\.checklist\.hint'\)"/],
    ['mcp/McpInfoDetail.vue', /<InfoTip :text="`\$\{t\('mcp\.detail\.enabledHint'\)\}/],
    ['mcp/McpServerDetail.vue', /<InfoTip :text="t\(isNew \? 'mcp\.detail\.enabledHintDraft'/],
    ['mcp/McpAiAddDialog.vue', /<InfoTip :text="`\$\{t\('mcp\.aiAdd\.runOnHint'\)\}\\n\$\{t\('mcp\.aiAdd\.privacy'\)\}`"/],
    ['slack/SlackAddDialog.vue', /<InfoTip :text="t\('slackThread\.add\.urlHint'\)"/],
    ['servers/ServerDetail.vue', /<InfoTip v-if="!isKey" :text="t\('servers\.editor\.agentHint'\)"/],
    ['servers/ServerDetail.vue', /<InfoTip :text="t\('servers\.editor\.keyCopiedHint'\)"/],
    ['servers/ServerDetail.vue', /<InfoTip :text="t\('servers\.editor\.passphraseHint'\)"/],
    ['items/ItemBody.vue', /<InfoTip :text="t\('item\.hint'/],
    ['secretNotes/SecretNotesPane.vue', /<InfoTip :text="t\('secretNotes\.editHint'/],
  ]
  for (const [file, re] of tips) assert.match(read(`../../components/${file}`), re, file)
  // The visible paragraphs are gone.
  assert.doesNotMatch(read('../../components/projects/ProjectServers.vue'), /<p[^>]*>\s*\{\{ t\('projectDetail\.toolPermsIntro'\)/)
  assert.doesNotMatch(read('../../components/projects/ProjectList.vue'), /<p[^>]*>\{\{ t\('projects\.dotsLegend'\)/)
  assert.doesNotMatch(read('../../components/mcp/McpAiAddDialog.vue'), /<p[^>]*>\{\{ t\('mcp\.aiAdd\.privacy'\)/)
  // Restating the label: dropped (library subtitle) or moved to the control's title (Slack send).
  assert.doesNotMatch(read('../../views/LibraryView.vue'), /library\.subtitle/)
  assert.match(read('../../components/slack/SlackReplyComposer.vue'), /:title="autoTranslate \? t\('slackThread\.reply\.hint'\)/)
  // Keys nothing uses any more are deleted in both languages.
  for (const lang of ['en', 'vi']) {
    assert.equal(json(`../../i18n/locales/${lang}/mcp.json`).detail.perProject, undefined)
    assert.equal(json(`../../i18n/locales/${lang}/projectDetail.json`).model.followText, undefined)
  }
})
