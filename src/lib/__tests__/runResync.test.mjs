// RT-16 (#19): "Re-sync from transcript" in the session menu (History row, header ⋯, Board card).
// Run: node --test src/lib/__tests__/runResync.test.mjs
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')
const items = read('../../components/RunActionItems.vue')
const template = items.slice(items.indexOf('<template>'))
const history = read('../../components/sessions/useRunHistoryList.ts')
const board = read('../../components/board/useBoardRunActions.ts')

test('the item shows for a session with a transcript that is not running', () => {
  assert.match(
    template,
    /v-if="run\.run_type === 'session' && run\.session_id && run\.status !== 'running'"[\s\S]*?@click="emit\('resync'\)"[\s\S]*?t\('run\.resync\.action'\)/,
  )
  assert.match(items, /resync: \[\]/)
})

test('both menus ask first, then call resync_run and drop the live copy', () => {
  for (const src of [history, board]) {
    assert.match(src, /resync: \(\) =>/)
    assert.match(src, /confirm\(resyncConfirmOptions\(t\)\)/)
    assert.match(src, /await resyncRun\(run\.id\)[\s\S]{0,120}?live\.discard\(run\.id\)/)
  }
  assert.match(read('../runResync.ts'), /invoke<\{ transcript_path: string \}>\('resync_run', \{ runId \}\)/)
})

test('strings exist in en and vi', () => {
  for (const lang of ['en', 'vi']) {
    const r = JSON.parse(read(`../../i18n/locales/${lang}/run.json`)).resync
    for (const k of ['action', 'confirmTitle', 'confirmMessage', 'confirm', 'done']) assert.ok(r[k], `${lang} ${k}`)
  }
})
