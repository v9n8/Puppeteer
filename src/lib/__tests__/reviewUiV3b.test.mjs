// UI review v3 fixes (UR-xx), continued: source-level checks of template rules that have
// no pure function to test, in the style of reviewUiV3.test.mjs.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

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
