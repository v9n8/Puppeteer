// UI review v3 fixes (UR-xx), continued: source-level checks of template rules that have
// no pure function to test, in the style of reviewUiV3.test.mjs.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { draftQuoteLink } from '../quote.ts'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')

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
