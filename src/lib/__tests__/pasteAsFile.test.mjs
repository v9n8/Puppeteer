// RT-26 (#16): a paste longer than the project's threshold becomes a `.txt` attachment.
// Run: node --test src/lib/__tests__/pasteAsFile.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  DEFAULT_PASTE_AS_FILE, PASTE_DEFAULT_MAX_CHARS, PASTE_MAX_MAX_CHARS,
  isInvertPasteKey, parseMaxChars, pasteAction, pasteFileName, pasteSetting, shouldPasteAsFile,
} from '../pasteAsFile.ts'

test('shouldPasteAsFile: only a paste longer than the threshold', () => {
  const s = { enabled: true, max_chars: 2000 }
  assert.equal(shouldPasteAsFile('a'.repeat(1999), s), false)
  assert.equal(shouldPasteAsFile('a'.repeat(2000), s), false, 'exactly the threshold stays text')
  assert.equal(shouldPasteAsFile('a'.repeat(2001), s), true)
})

test('shouldPasteAsFile: turned off never makes a file', () => {
  assert.equal(shouldPasteAsFile('a'.repeat(50_000), { enabled: false, max_chars: 1 }), false)
})

test('shouldPasteAsFile: counts String.length (UTF-16 units)', () => {
  // 2 emoji = 4 code units: over a threshold of 3.
  assert.equal(shouldPasteAsFile('😀😀', { enabled: true, max_chars: 3 }), true)
  assert.equal(shouldPasteAsFile('ệệệ', { enabled: true, max_chars: 3 }), false)
})

test('shouldPasteAsFile: missing or bad setting uses the backend default', () => {
  assert.deepEqual(pasteSetting(undefined), DEFAULT_PASTE_AS_FILE)
  assert.deepEqual(pasteSetting({ enabled: false, max_chars: 0 }), { enabled: false, max_chars: PASTE_DEFAULT_MAX_CHARS })
  assert.deepEqual(pasteSetting({ enabled: true, max_chars: 1.5 }), DEFAULT_PASTE_AS_FILE)
  assert.equal(shouldPasteAsFile('a'.repeat(2001), null), true)
  assert.equal(shouldPasteAsFile('a'.repeat(2000), { max_chars: -5 }), false)
})

test('parseMaxChars: a whole number from 1, never empty', () => {
  assert.equal(parseMaxChars('1'), 1)
  assert.equal(parseMaxChars(' 2500 '), 2500)
  assert.equal(parseMaxChars(300), 300)
  assert.equal(parseMaxChars(String(PASTE_MAX_MAX_CHARS)), PASTE_MAX_MAX_CHARS)
  for (const bad of ['', '   ', '0', '-3', '1.5', '1e3', 'abc', null, undefined, String(PASTE_MAX_MAX_CHARS + 1)]) {
    assert.equal(parseMaxChars(bad), null, String(bad))
  }
})

test('pasteFileName: last part of either separator', () => {
  assert.equal(pasteFileName('/a/pastes/r1/paste-1.txt'), 'paste-1.txt')
  assert.equal(pasteFileName('C:\\x\\paste-2.txt'), 'paste-2.txt')
})

test('composer: the paste handler hands text to usePasteAsFile (wiring)', () => {
  const src = readFileSync(new URL('../../components/sessions/useRunComposer.ts', import.meta.url), 'utf8')
  assert.match(src, /usePasteAsFile\(/)
  assert.match(src, /pasteAsFile\.onPaste\(e\)/)
  const hook = readFileSync(new URL('../../components/sessions/usePasteAsFile.ts', import.meta.url), 'utf8')
  assert.match(hook, /invoke<string>\('save_pasted_text'/)
  assert.match(hook, /projectPageQuery\([^)]*'session'\)/)
})

test('the shortcut pastes the other way whatever the length', () => {
  const on = { enabled: true, max_chars: 2000 }
  const off = { enabled: false, max_chars: 2000 }
  const long = 'x'.repeat(50_000)
  const short = 'x'.repeat(20)
  const run = { inverted: true, hasRun: true }
  // Setting on: even 50 000 characters go into the draft.
  assert.equal(pasteAction(long, on, run), 'draft')
  // Setting off: even 20 characters become a file.
  assert.equal(pasteAction(short, off, run), 'file')
  // No run yet: a file cannot be made, so the draft, with a reason.
  assert.equal(pasteAction(short, off, { inverted: true, hasRun: false }), 'draft_no_run')
  assert.equal(pasteAction(long, on, { inverted: true, hasRun: false }), 'draft')
})

test('a plain paste keeps the length rule, and needs a run for a file', () => {
  const on = { enabled: true, max_chars: 2000 }
  const plain = { inverted: false, hasRun: true }
  assert.equal(pasteAction('x'.repeat(2001), on, plain), 'file')
  assert.equal(pasteAction('x'.repeat(2000), on, plain), 'draft')
  assert.equal(pasteAction('x'.repeat(2001), { enabled: false, max_chars: 2000 }, plain), 'draft')
  assert.equal(pasteAction('x'.repeat(2001), on, { inverted: false, hasRun: false }), 'draft')
})

test('the shortcut is Cmd+Shift+V on macOS and Ctrl+Shift+V elsewhere', () => {
  const key = (extra) => ({ metaKey: false, ctrlKey: false, shiftKey: true, altKey: false, key: 'V', code: 'KeyV', isComposing: false, ...extra })
  assert.equal(isInvertPasteKey(key({ metaKey: true }), true), true)
  assert.equal(isInvertPasteKey(key({ ctrlKey: true }), true), false)
  assert.equal(isInvertPasteKey(key({ ctrlKey: true }), false), true)
  assert.equal(isInvertPasteKey(key({ metaKey: true }), false), false)
  // Plain paste, Option+Shift, another key, or an IME composition: not the shortcut.
  assert.equal(isInvertPasteKey(key({ metaKey: true, shiftKey: false }), true), false)
  assert.equal(isInvertPasteKey(key({ metaKey: true, altKey: true }), true), false)
  assert.equal(isInvertPasteKey(key({ metaKey: true, key: 'C' }), true), false)
  // The character, not the physical key: on a Latin layout whose V sits elsewhere (Dvorak),
  // the V character matches and the key under the QWERTY V does not.
  assert.equal(isInvertPasteKey(key({ metaKey: true, key: 'v', code: 'KeyK' }), true), true)
  assert.equal(isInvertPasteKey(key({ metaKey: true, key: 'k', code: 'KeyV' }), true), false)
  // A non-Latin layout gives no Latin letter: the physical V key counts.
  assert.equal(isInvertPasteKey(key({ metaKey: true, key: 'м', code: 'KeyV' }), true), true)
  assert.equal(isInvertPasteKey(key({ metaKey: true, key: 'м', code: 'KeyC' }), true), false)
  assert.equal(isInvertPasteKey(key({ metaKey: true, isComposing: true }), true), false)
})
