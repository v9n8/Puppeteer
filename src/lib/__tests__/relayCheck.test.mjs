// Unit tests for lib/relayCheck.ts (Settings → Devices "Check relay" result line).
// Run: node --test src/lib/__tests__/relayCheck.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { relayCheckLine } from '../relayCheck.ts'

const t = (key, p) => `${key.split('.').pop()}|${JSON.stringify(p)}`
const base = { reachable: true, connect_ms: 42, token: 'accepted', error_code: null, error: null, reply_wait_secs: 10 }

test('each outcome picks its own message with the observed values', () => {
  assert.equal(relayCheckLine(base, t), 'accepted|{"ms":42}')
  assert.equal(relayCheckLine({ ...base, token: 'rejected', error_code: 'auth_failed' }, t), 'rejected|{"ms":42,"code":"auth_failed"}')
  assert.equal(relayCheckLine({ ...base, token: 'no_reply' }, t), 'noReply|{"ms":42,"s":10}')
  assert.equal(relayCheckLine({ ...base, token: 'no_reply', error: 'closed' }, t), 'noReplyWhy|{"ms":42,"error":"closed"}')
  assert.equal(relayCheckLine({ ...base, token: 'not_checked' }, t), 'notChecked|{"ms":42}')
})

test('an unreachable relay shows the real error', () => {
  const r = { ...base, reachable: false, connect_ms: null, token: 'not_checked', error: 'HTTP 404' }
  assert.equal(relayCheckLine(r, t), 'failed|{"error":"HTTP 404"}')
})
