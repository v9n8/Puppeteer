// Run: node --test src/lib/__tests__/windowCreateGuard.test.mjs
// A failed window creation must reach the user and make the open function throw,
// so detachScreen puts the screen back. Source check: the helper imports app aliases.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const FILES = [
  'focusWindow', 'itemListWindow', 'slackWindow', 'ganttWindow', 'prInboxWindow',
  'fileTransferWindow', 'secretNotesWindow', 'settingsWindow', 'calendarWindow',
]
const src = (name) => readFileSync(new URL(`../${name}.ts`, import.meta.url), 'utf8')

for (const name of FILES) {
  test(`${name} awaits creation through the shared guard`, () => {
    const code = src(name)
    assert.match(code, /await awaitWindowCreated\(win, '/)
    assert.doesNotMatch(code, /tauri:\/\/error/)
  })
}

test('the guard toasts, logs and rethrows', () => {
  const code = src('windowCreateGuard')
  assert.match(code, /nav\.windowOpenFailed/)
  assert.match(code, /console\.error/)
  assert.match(code, /throw e/)
})
