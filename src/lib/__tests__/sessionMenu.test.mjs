// RT-17 (#21): the session menu (RunActionItems) — History row, header ⋯, Board card.
// Run: node --test src/lib/__tests__/sessionMenu.test.mjs
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')
const json = (p) => JSON.parse(read(p))
const items = read('../../components/RunActionItems.vue')
const template = items.slice(items.indexOf('<template>'))
const history = read('../../components/sessions/useRunHistoryList.ts')
const board = read('../../components/board/useBoardRunActions.ts')

test('"View log file", "Export transcript" and the duplicate "Copy log file path" are gone', () => {
  for (const key of ['run.viewLogFile', 'run.exportTranscript', 'run.copyLogPath']) {
    assert.ok(!template.includes(key), key)
  }
  assert.doesNotMatch(items, /viewLog: \[\]|exportTranscript: \[\]|copyWorkDir: \[\]|revealWorkDir: \[\]/)
  for (const src of [history, board]) {
    assert.doesNotMatch(src, /viewLog:|exportTranscript:|copyWorkDir:|revealWorkDir:/)
  }
})

test('"Copy path" and "Reveal in Finder" point at the session file', () => {
  assert.match(template, /<DropdownItem @click="emit\('copyLogPath'\)">[\s\S]*?t\('run\.copyWorkDir'\)/)
  assert.match(template, /<DropdownItem @click="emit\('revealLog'\)">[\s\S]*?t\(revealKey\)/)
  for (const src of [history, board]) {
    assert.match(src, /copyLogPath: \(\) =>/)
    assert.match(src, /revealLog: \(\) =>/)
  }
})

test('"Open in new window" stays only where the host asks for it (the Board)', () => {
  assert.match(template, /<DropdownItem v-if="canPopout && !isSessionWindow" @click="emit\('popout'\)">/)
  assert.match(board, /canPopout: true/)
  assert.doesNotMatch(history, /canPopout/)
})

test('Pin is called "Pin" / "Ghim"', () => {
  assert.equal(json('../../i18n/locales/en/run.json').pinToTop, 'Pin')
  assert.equal(json('../../i18n/locales/vi/run.json').pinToTop, 'Ghim')
  assert.equal(json('../../i18n/locales/en/run.json').unpinFromTop, 'Unpin')
  assert.equal(json('../../i18n/locales/vi/run.json').unpinFromTop, 'Bỏ ghim')
})
