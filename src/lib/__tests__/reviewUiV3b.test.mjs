// UI review v3 fixes (UR-xx), continued: source-level checks of template rules that have
// no pure function to test, in the style of reviewUiV3.test.mjs.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { draftQuoteLink } from '../quote.ts'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')

test('UR-01: deleting every session also clears the open one when the trash event arrives', () => {
  const src = read('../../components/sessions/useRunSession.ts')
  assert.match(src, /listen\(EVT_TRASH_CHANGED, async \(\) => \{\s*await runsStore\.fetchRuns\(projectId\.value\)[\s\S]*?!runsStore\.runs\.some\(r => r\.id === openId\)[\s\S]*?clearActiveRunState\(\)[\s\S]*?router\.replace\(/)
  assert.match(read('../../components/sessions/useRunHistoryList.ts'), /runActionHandlers, handleRefInput, clearActiveRunState,/)
})

test('UR-47: line gutter follows the digit count and the floating bars hang off a zero-height anchor', () => {
  const src = read('../../components/FileViewer.vue')
  assert.match(src, /const gutterWidth = computed\(\(\) => `calc\(\$\{Math\.max\(String\(lines\.value\.length\)\.length, 2\)\}ch \+ 1\.25rem\)`\)/)
  assert.match(src, /:style="\{ width: gutterWidth \}"/)
  assert.doesNotMatch(src, /tabular-nums[^"]*\bw-12\b/)
  assert.doesNotMatch(src, /absolute top-14 right-4/)
  assert.match(src, /relative z-20 h-0 shrink-0[\s\S]*<FindBar[\s\S]*absolute top-2 right-4[\s\S]*<CompareFilePicker[\s\S]*absolute top-2 right-4/)
})

test('UR-45: frames shrink to their content and the editors follow the real theme', () => {
  assert.match(read('../../components/fileTransfer/FileTransferScreen.vue'), /class="max-h-56 shrink-0"/)
  assert.match(read('../../components/remote/RemoteFileDialog.vue'), /flex flex-col max-h-\[60vh\] min-h-40/)
  assert.doesNotMatch(read('../../components/settings/MemorySettings.vue'), /h-\[420px\]/)
  for (const file of ['../../components/settings/MemorySettings.vue', '../../components/library/LibraryEntryDetail.vue']) {
    const src = read(file)
    assert.match(src, /<SkillEditor v-model="content" :is-dark="isDark"/, file)
    assert.match(src, /const isDark = useIsDark\(\)/, file)
    assert.doesNotMatch(src, /:is-dark="true"/, file)
  }
  // The editor swaps its theme live instead of only reading the prop at mount.
  const editor = read('../../components/SkillEditor.vue')
  assert.match(editor, /themeSlot\.reconfigure\(dark \? oneDark : \[\]\)/)
  // The conflict banner's three buttons sit on their own row below the message.
  const lib = read('../../components/library/LibraryEntryDetail.vue')
  const banner = lib.slice(lib.indexOf('<Banner v-if="conflict"'), lib.indexOf('</Banner>', lib.indexOf('<Banner v-if="conflict"')))
  assert.match(banner, /<div class="mt-2 flex flex-wrap items-center gap-1">/)
  assert.doesNotMatch(banner, /#actions/)
})

test('UR-17: the project tab menu says "Project settings" and sets the icon under the colour row', () => {
  const src = read('../../components/WorkspaceTabs.vue')
  assert.match(src, /projectsStore\.updateAppearance\(project, \{ icon \}\)/)
  assert.match(src, /v-for="icon in PROJECT_ICONS"/)
  assert.match(src, /t\('misc\.tabs\.clearIcon'\)/)
  assert.ok(src.indexOf("t('projectDetail.colorLabel')") < src.indexOf("t('projectDetail.iconLabel')"))
  assert.equal(json('../../i18n/locales/en/misc.json').tabs.viewProject, 'Project settings')
  assert.equal(json('../../i18n/locales/vi/misc.json').tabs.viewProject, 'Cài đặt Project')
  for (const lang of ['en', 'vi']) assert.ok(json(`../../i18n/locales/${lang}/misc.json`).tabs.clearIcon, lang)
})

test('UR-33: session file tabs drag to reorder within their group; Conversation stays put', () => {
  const src = read('../../components/sessions/SessionTabBar.vue')
  const template = src.slice(src.indexOf('<template>'), src.indexOf('<style'))
  // One sortable per group, each committing through the store, which saves it.
  assert.match(src, /itemSelector: '\[data-drag-group="pinned"\]'/)
  assert.match(src, /itemSelector: '\[data-drag-group="rest"\]'/)
  assert.match(src, /tabsStore\.move\(runId\.value, tab\.id, to\)/)
  assert.match(read('../../stores/sessionTabs.ts'), /put\(runId, moveTab\(/)
  // File tabs carry the group and start a drag; the Conversation tab does neither.
  assert.match(template, /:data-drag-group="tab\.pinned \? 'pinned' : 'rest'"/)
  assert.match(template, /@pointerdown="startDrag\(tab, i, \$event\)"/)
  const conversation = template.slice(template.indexOf('data-tab-id="conversation"'), template.indexOf('v-for="(tab, i) in shownTabs"'))
  assert.doesNotMatch(conversation, /pointerdown|data-drag-group/)
  // The click ending a drag does not select; pressing pin / close never starts a drag.
  assert.match(src, /if \(pinnedDrag\.clickWasDrag\(\) \|\| restDrag\.clickWasDrag\(\)\) return/)
  assert.equal(template.match(/@pointerdown\.stop/g)?.length, 2)
})

test('UR-33: tab button hover uses foreground/10, visible on the selected tab in dark', () => {
  const css = read('../../components/sessions/SessionTabBar.vue').split('<style scoped>')[1]
  const hover = css.slice(css.indexOf('.session-tab-btn:hover'))
  assert.match(hover, /background: hsl\(var\(--foreground\) \/ 0\.1\)/)
})

const json = (p) => JSON.parse(read(p))

test('UR-35: Settings pages keep no explanation paragraph; kept text sits in an InfoTip and the dropped keys are gone', () => {
  const s = '../../components/settings/'
  // BỎ: intros and descriptions that only repeat the label are not rendered at all.
  assert.doesNotMatch(read(s + 'GeneralSettings.vue'), /general\.intro|colorThemeHint|terminalAppDesc/)
  assert.doesNotMatch(read(s + 'ShortcutsSettings.vue'), /shortcuts\.intro/)
  assert.doesNotMatch(read(s + 'LimitsSettings.vue'), /sessions\.intro|<p [^>]*>\{\{ tabHint/)
  assert.doesNotMatch(read('../../components/CommandPaletteSettings.vue'), /palette\.settings\.intro/)
  assert.match(read(s + 'SettingsSectionHead.vue'), /<InfoTip v-if="intro"/)
  // ICON: the explanation moves into an InfoTip next to its heading.
  assert.match(read(s + 'LimitsSettings.vue'), /<InfoTip :text="tabHint" \/>/)
  assert.match(read(s + 'FlagsSettings.vue'), /<InfoTip :text="t\('sessionFlags\.settings\.footer'\)" \/>/)
  assert.match(read('../../components/KeymapSettings.vue'), /<InfoTip :text="t\('keymap\.help', systemKeys\)" \/>/)
  assert.match(read(s + 'ModelSettings.vue'), /<InfoTip :text="t\('settings\.model\.description'\)" \/>/)
  assert.match(read('../../views/SettingsView.vue'), /<InfoTip :text="t\('settings\.mcp\.description'\)" \/>/)
  // Cloud accounts keep one short line plus the full warning in an InfoTip.
  const files = {
    aws: `${s}AwsAccountsSettings.vue`,
    github: `${s}GithubAccountsSettings.vue`,
    gitlab: `${s}GitlabAccountsSettings.vue`,
    azure: '../../components/AzureAccountsSettings.vue',
  }
  for (const [k, file] of Object.entries(files)) {
    const src = read(file)
    assert.match(src, new RegExp(`settings\\.${k}\\.isolationShort`))
    assert.match(src, new RegExp(`<InfoTip :text="t\\('settings\\.${k}\\.isolationWarning'\\)" />`))
  }
  for (const lang of ['en', 'vi']) {
    const d = json(`../../i18n/locales/${lang}/settings.json`)
    assert.equal(d.general.intro, undefined, lang)
    assert.equal(d.general.colorThemeHint, undefined, lang)
    assert.equal(d.general.terminalAppDesc, undefined, lang)
    assert.equal(d.ai.intro, undefined, lang)
    assert.equal(d.engines.intro, undefined, lang)
    assert.equal(d.shortcuts.intro, undefined, lang)
    assert.equal(d.sessions.intro, undefined, lang)
    assert.equal(d.sessions.notifyDescriptions.done, undefined, lang)
    assert.equal(d.sessions.notifyDescriptions.sound, undefined, lang)
    for (const k of Object.keys(files)) assert.ok(d[k].isolationShort, `${lang} ${k}`)
  }
})

test('UR-29: a quote above the composer leads to the quoted message or file, or says why not', () => {
  const entry = {
    id: 'q1', kind: 'entry', excerpt: 'quoted', run_id: 'r1', source_entry_id: 'm1', block_index: 2, start: 4, end: 10,
  }
  const shown = (id) => id === 'm1'
  const toMessage = draftQuoteLink(entry, shown, false)
  assert.equal(toMessage.to, 'message')
  // jumpToQuote gets the same anchor a sent card would carry.
  assert.equal(toMessage.quote.source_entry_id, 'm1')
  assert.equal(toMessage.quote.block_index, 2)
  assert.equal(toMessage.quote.start_offset, 4)
  assert.equal(toMessage.quote.end_offset, 10)
  // Off screen: not loaded while the history has more to load, gone otherwise.
  assert.deepEqual(draftQuoteLink(entry, () => false, true), { to: 'none', reason: 'notLoaded' })
  assert.deepEqual(draftQuoteLink(entry, () => false, false), { to: 'none', reason: 'gone' })
  // A file quote opens at its first line; plain text leads nowhere, without a reason.
  const file = { id: 'q2', kind: 'file', excerpt: 'x', path: 'src/a.ts', lines: { start: 12, end: 14 } }
  assert.deepEqual(draftQuoteLink(file, shown, false), { to: 'file', path: 'src/a.ts', line: 12 })
  assert.deepEqual(draftQuoteLink({ id: 'q3', kind: 'file', excerpt: 'x', path: 'b.md' }, shown, false), { to: 'file', path: 'b.md', line: null })
  assert.deepEqual(draftQuoteLink({ id: 'q4', kind: 'text', excerpt: 'x' }, shown, false), { to: 'none', reason: null })

  // The row is a button only when it leads somewhere; the composer follows it
  // with the log's own jump and the file opener.
  const block = read('../../components/sessions/ComposerQuoteBlock.vue')
  assert.match(block, /:is="linkOf\(q\) \? 'button' : 'div'"/)
  assert.match(block, /follow: \[link: DraftQuoteLink\]/)
  assert.match(block, /run\.quotes\.sourceNotLoaded/)
  assert.match(block, /streamLog\.quotes\.sourceGone/)
  const composer = read('../../components/sessions/RunComposer.vue')
  assert.match(composer, /@follow="followQuote"/)
  assert.match(composer, /openFile\(link\.path, link\.line\)/)
  assert.match(composer, /jumpToQuote\(link\.quote\)/)
  for (const lang of ['en', 'vi']) assert.ok(json(`../../i18n/locales/${lang}/run.json`).quotes.sourceNotLoaded, lang)
})

test('UR-44: menu group headings use DropdownLabel, sentence case, no tracked upper-case', () => {
  const label = read('../../components/ui/DropdownLabel.vue')
  assert.match(label, /text-2xs font-medium text-muted-foreground/)
  assert.doesNotMatch(label, /uppercase|tracking-/)
  assert.match(read('../../components/ui/index.ts'), /DropdownLabel/)
  const hosts = [
    'AppSelect.vue', 'sessions/HistoryFlagFilter.vue', 'sessions/LinkedSessionsMenu.vue',
    'sessions/SessionTabBar.vue', 'sessions/RunLogPane.vue', 'CapturePicker.vue', 'CommandPalette.vue',
    'ProjectSwitcher.vue', 'prInbox/PrInboxFiltersPopover.vue', 'prInbox/PrInboxDisplayPopover.vue',
  ]
  for (const h of hosts) {
    const src = read(`../../components/${h}`)
    assert.match(src, /<DropdownLabel/, h)
    assert.doesNotMatch(src, /uppercase tracking-w/, h)
  }
  assert.match(read('../../views/CalendarView.vue'), /<DropdownLabel class="mb-1.5">/)
})

test('UR-42: header refresh buttons are icon-only with title and aria-label; the change kind never shrinks', () => {
  const views = [
    ['../../views/BoardView.vue', 'board.refresh'],
    ['../../views/PrInboxView.vue', 'common.refresh'],
    ['../../views/IssuesGanttView.vue', 'common.refresh'],
    ['../../views/SchedulesView.vue', 'schedules.refresh'],
    ['../../views/TrashView.vue', 'trash.refresh'],
    ['../../components/sessions/ChangedFilesTab.vue', 'run.changedFiles.reload'],
  ]
  for (const [file, key] of views) {
    const src = read(file)
    assert.match(src, new RegExp(`:title="t\\('${key}'\\)"\\s+:aria-label="t\\('${key}'\\)"`), file)
    // The label may only appear as an attribute, never as visible text (PrInbox keeps one in its error retry).
    if (!file.includes('PrInbox')) assert.doesNotMatch(src, new RegExp(`\\{\\{ t\\('${key}'\\) \\}\\}`), file)
  }
  const changed = read('../../components/sessions/ChangedFilesTab.vue')
  assert.match(changed, /min-w-8 shrink-0 whitespace-nowrap/)
  assert.doesNotMatch(changed, /class="w-8 shrink-0/)
})

test('UR-41: destructive resets and stops ask first; the sort note is an InfoTip', () => {
  const asks = [
    ['KeymapSettings.vue', 'keymap.resetAllAsk'],
    ['projects/ProjectBoard.vue', 'projectDetail.unlinkAsk'],
    ['running/RunningSessionsTab.vue', 'processes.running.confirm.stopOneMessage'],
    ['remote/RemotePushSettings.vue', 'remote.push.removeAsk'],
    ['fileTransfer/FileTransferQueue.vue', 'fileTransfer.queue.cancelAllAsk'],
    ['CommandPaletteSettings.vue', 'palette.settings.resetOrderAsk'],
    ['settings/GeneralSettings.vue', 'settings.general.resetTextAsk'],
    ['sessions/RunMcpSection.vue', 'run.mcpResetAsk'],
  ]
  for (const [file, key] of asks) {
    const src = read(`../../components/${file}`)
    assert.match(src, /useConfirm\(\)/, file)
    assert.ok(src.includes(`t('${key}'`), `${file} asks with ${key}`)
    // Cancel leaves the state alone: early return, or the action only runs when confirmed.
    assert.match(src, /if \(!ok\) return|if \(ok\) (await save|emit)/, file)
  }
  // The queue emits cancelAll only after the answer.
  assert.match(read('../../components/fileTransfer/FileTransferQueue.vue'), /@click="askCancelAll"/)
  // The Session row and the worker row go through the same ask.
  const running = read('../../components/running/RunningSessionsTab.vue')
  assert.match(running, /stopOne\(w, true\)/)
  assert.match(running, /<InfoTip :text="t\('processes.running.filter.sort'\)"/)
})

test('UR-39: hover and pinned states differ from the surface behind them', () => {
  const composer = read('../../components/sessions/RunComposer.vue')
  assert.equal(composer.match(/hover:bg-foreground\/10/g)?.length, 4)
  assert.doesNotMatch(composer, /rounded-sm flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-background/)
  const ring = read('../../components/UsageRing.vue')
  assert.match(ring, /hover:bg-accent hover:text-foreground/)
  assert.match(ring, /'bg-accent text-foreground': pinned/)
  assert.match(read('../../components/projects/ProjectList.vue'), /cursor-grab[^"]*hover:bg-foreground\/10/)
})

test('UR-37: remaining screens put their explanations behind an InfoTip and drop the restating lines', () => {
  const tips = [
    ['views/SchedulesView.vue', 'schedules.hint'],
    ['views/TrashView.vue', 'trash.hint'],
    ['views/TrashView.vue', 'trash.search.hint'],
    ['views/TrashView.vue', 'trash.content.hint'],
    ['components/running/BackgroundProcessesTab.vue', 'processes.subtitle'],
    ['components/running/BackgroundProcessesTab.vue', 'processes.sections.servicesHint'],
    ['components/processes/DevServersCard.vue', 'processes.devServers.hint'],
    ['views/StatsView.vue', 'stats.storage.note'],
    ['components/prInbox/PrInboxBotAccountsModal.vue', 'prInbox.botAccounts.hint'],
    ['components/ImageCompareView.vue', 'files.compare.hint'],
    ['components/fileTransfer/FileTransferScreen.vue', 'fileTransfer.hint'],
    ['components/focus/FocusAiDialogs.vue', 'focus.ai.pickProjectHint'],
    ['components/calendar/EventEditorModal.vue', 'calendar.editor.attendeesEditHint'],
    ['components/calendar/EventEditorModal.vue', 'calendar.editor.remindersEditHint'],
    ['components/calendar/EventEditorModal.vue', 'calendar.editor.recurrenceEditHint'],
  ]
  for (const [file, key] of tips) {
    const src = read(`../../${file}`)
    const k = key.replace(/\./g, '\\.')
    assert.ok(new RegExp(`<InfoTip[^>]*t\\('${k}'`).test(src), `${file} shows ${key} in an InfoTip`)
    assert.doesNotMatch(src, new RegExp(`<p[^>]*>\\s*\\{\\{ t\\('${k}'`), `${file} no longer prints ${key} as a paragraph`)
  }
  // The Stats plan sentences are joined from their i18n pieces for the tip.
  const stats = read('../../views/StatsView.vue')
  assert.match(stats, /<InfoTip :text="claudePlanHint"/)
  assert.match(stats, /<InfoTip :text="codexPlanHint"/)
  // The services header toggle is a button, so its tip sits beside it, not inside it.
  assert.match(read('../../components/running/BackgroundProcessesTab.vue'), /<\/button>\s*<InfoTip :text="t\('processes.sections.servicesHint'\)"/)
  // Lines that only restated the switch label are gone together with their keys.
  const dropped = [
    ['views/CalendarView.vue', 'calendar.view.lunarHint'],
    ['views/CalendarView.vue', 'calendar.view.eventRemindersHint'],
    ['components/prInbox/PrInboxDisplayPopover.vue', 'prInbox.display.hideBotsHint'],
    ['components/flags/FlagColorPopover.vue', 'sessionFlags.color.footer'],
  ]
  for (const [file, key] of dropped) {
    assert.ok(!read(`../../${file}`).includes(key), `${file} no longer uses ${key}`)
    const [ns, ...path] = key.split('.')
    for (const l of ['en', 'vi']) {
      let node = JSON.parse(read(`../../i18n/locales/${l}/${ns}.json`))
      for (const p of path) node = node?.[p]
      assert.equal(node, undefined, `${l}/${ns}.json dropped ${key}`)
    }
  }
})

test('UR-36: the project accounts card imports InfoTip because its template uses it', () => {
  const src = read('../../components/projects/ProjectAccounts.vue')
  assert.match(src, /<InfoTip\b/)
  assert.match(src, /import \{[^}]*\bInfoTip\b[^}]*\} from '@\/components\/ui'/)
})

test('UR-17: the tab menu icon row wraps inside the menu instead of widening it', () => {
  const src = read('../../components/WorkspaceTabs.vue')
  assert.match(src, /class="[^"]*\bw-0 min-w-full\b[^"]*"\s*>\s*<button\s+v-for="icon in PROJECT_ICONS"/)
})
