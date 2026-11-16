// Unit tests for lib/updateStatus.ts (the Settings › About "Updates" card).
// Run: node --test src/lib/__tests__/updateStatus.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { updateLine, updateStatusFrom } from '../updateStatus.ts'

const t = (key, params = {}) => `${key}${Object.keys(params).length ? JSON.stringify(params) : ''}`
const fmt = (ms) => `@${ms}`
const base = { enabled: true, installing: false, lastCheckedAt: null, status: 'idle', version: null, error: null }
const line = (over) => updateLine({ ...base, ...over }, t, fmt)

test('updateStatusFrom: each answer of check_update, and a thrown error', () => {
  assert.deepEqual(updateStatusFrom({ status: 'up_to_date' }), { status: 'up_to_date', version: null, error: null })
  assert.deepEqual(updateStatusFrom({ status: 'unsupported' }), { status: 'unsupported', version: null, error: null })
  assert.deepEqual(
    updateStatusFrom({ status: 'available', version: '1.2.0', current_version: '1.1.0' }),
    { status: 'available', version: '1.2.0', error: null },
  )
  assert.deepEqual(updateStatusFrom({ error: new Error('offline') }), { status: 'error', version: null, error: 'Error: offline' })
  assert.deepEqual(updateStatusFrom({ error: 'no manifest' }), { status: 'error', version: null, error: 'no manifest' })
})

test('updateLine: not checked by hand yet offers the check', () => {
  assert.deepEqual(line({}), { text: 'update.card.idle', action: 'check' })
  assert.deepEqual(line({ lastCheckedAt: 5 }), { text: 'update.card.idleLast{"time":"@5"}', action: 'check' })
})

test('updateLine: checking locks the button', () => {
  assert.equal(line({ status: 'checking' }).action, 'checking')
})

test('updateLine: up to date, available, error', () => {
  assert.deepEqual(line({ status: 'up_to_date', lastCheckedAt: 9 }), { text: 'update.card.upToDate{"time":"@9"}', action: 'check' })
  assert.deepEqual(line({ status: 'available', version: '1.2.0' }), { text: 'update.available{"version":"1.2.0"}', action: 'install' })
  assert.equal(line({ status: 'available', version: '1.2.0', installing: true }).action, 'installing')
  assert.deepEqual(line({ status: 'error', error: 'offline' }), { text: 'update.card.error{"error":"offline"}', action: 'retry' })
})

test('updateLine: switched off and unsupported builds offer nothing', () => {
  assert.deepEqual(line({ enabled: false, status: 'available', version: '1.2.0' }), { text: 'update.card.disabled', action: 'none' })
  assert.deepEqual(line({ status: 'unsupported' }), { text: 'update.card.unsupported', action: 'none' })
})
