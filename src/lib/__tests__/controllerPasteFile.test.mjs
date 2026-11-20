// RT-26 (#16) on the remote controller: a long text paste is saved on the Host
// (`save_pasted_text`) and attached by its Host path. Failure (a refusal, no ack
// in time, not sent) leaves the text to go into the draft.
// Run: node --test src/lib/__tests__/controllerPasteFile.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  PASTE_ACK_TIMEOUT_MS, pasteChoice, pasteErrorKey, requestPasteFile, savePastedTextCmd,
} from '../../controller/pasteFile.ts'

const long = 'a'.repeat(2001)

test('pasteChoice: a long paste in an open run becomes a file', () => {
  assert.equal(pasteChoice({ hasImage: false, runId: 'r1', text: long, setting: undefined }), 'file')
  assert.equal(pasteChoice({ hasImage: false, runId: 'r1', text: 'a'.repeat(2000), setting: undefined }), 'text')
})

test('pasteChoice: no run, an image, empty text or turned off stays text', () => {
  assert.equal(pasteChoice({ hasImage: false, runId: null, text: long, setting: undefined }), 'text')
  assert.equal(pasteChoice({ hasImage: true, runId: 'r1', text: long, setting: undefined }), 'text')
  assert.equal(pasteChoice({ hasImage: false, runId: 'r1', text: '', setting: { enabled: true, max_chars: 0 } }), 'text')
  assert.equal(pasteChoice({ hasImage: false, runId: 'r1', text: long, setting: { enabled: false, max_chars: 1 } }), 'text')
})

test('pasteChoice: follows the project threshold', () => {
  assert.equal(pasteChoice({ hasImage: false, runId: 'r1', text: 'abcd', setting: { enabled: true, max_chars: 3 } }), 'file')
})

test('savePastedTextCmd: the wire shape the Host reads', () => {
  assert.deepEqual(savePastedTextCmd('r1', 'hi'), { kind: 'cmd', action: 'save_pasted_text', run_id: 'r1', text: 'hi' })
})

function fakeTimers() {
  const pending = new Map()
  let seq = 0
  return {
    pending,
    timers: {
      set: (fn, ms) => { const h = ++seq; pending.set(h, { fn, ms }); return h },
      clear: (h) => { pending.delete(h) },
    },
    fire() { for (const [h, { fn }] of [...pending]) { pending.delete(h); fn() } },
  }
}

test('requestPasteFile: an accepted ack with a path attaches it', async () => {
  const ft = fakeTimers()
  let sentCmd = null
  const r = requestPasteFile((cmd, onAck) => {
    sentCmd = cmd
    queueMicrotask(() => onAck(true, null, null, null, '/data/pastes/r1/paste-1.txt'))
    return true
  }, 'r1', long, { timers: ft.timers })
  assert.equal(ft.pending.size, 1)
  assert.deepEqual(await r, { ok: true, path: '/data/pastes/r1/paste-1.txt' })
  assert.equal(sentCmd.action, 'save_pasted_text')
  assert.equal(ft.pending.size, 0, 'the timer is cleared')
})

test('requestPasteFile: a refusal carries its reason', async () => {
  for (const reason of ['too_large', 'malformed', 'wrong_run']) {
    const r = await requestPasteFile((_c, onAck) => { onAck(false, reason); return true }, 'r1', long, { timers: fakeTimers().timers })
    assert.deepEqual(r, { ok: false, reason })
  }
})

test('requestPasteFile: accepted without a path is a failure', async () => {
  const r = await requestPasteFile((_c, onAck) => { onAck(true); return true }, 'r1', long, { timers: fakeTimers().timers })
  assert.deepEqual(r, { ok: false, reason: 'no_path' })
})

test('requestPasteFile: no ack before the timeout fails, a late ack is ignored', async () => {
  const ft = fakeTimers()
  let ack
  const r = requestPasteFile((_c, onAck) => { ack = onAck; return true }, 'r1', long, { timers: ft.timers })
  assert.equal([...ft.pending.values()][0].ms, PASTE_ACK_TIMEOUT_MS)
  ft.fire()
  ack(true, null, null, null, '/late.txt')
  assert.deepEqual(await r, { ok: false, reason: 'timeout' })
})

test('requestPasteFile: a command that could not leave fails at once', async () => {
  const ft = fakeTimers()
  const r = await requestPasteFile(() => false, 'r1', long, { timers: ft.timers })
  assert.deepEqual(r, { ok: false, reason: 'not_sent' })
  assert.equal(ft.pending.size, 0)
})

test('pasteErrorKey: too_large and timeouts have their own message', () => {
  assert.equal(pasteErrorKey('too_large'), 'pasteTooLarge')
  assert.equal(pasteErrorKey('timeout'), 'pasteTimeout')
  assert.equal(pasteErrorKey('host_ack_timeout'), 'pasteTimeout')
  assert.equal(pasteErrorKey('connection_lost'), 'pasteNotSent')
  assert.equal(pasteErrorKey('not_sent'), 'pasteNotSent')
  assert.equal(pasteErrorKey('wrong_run'), 'pasteFailed')
})
