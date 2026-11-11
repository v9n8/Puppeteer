// Unit tests for lib/engineAccounts.ts: the Claude + Codex account list and the
// sign-in steps of Settings → Engines & accounts (UX-42).
// Run: node --test src/lib/__tests__/engineAccounts.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { accountKey, accountRows, loginSteps, SHARED_CODEX_KEY } from '../engineAccounts.ts'

const acc = (id, extra = {}) => ({
  id, label: id.toUpperCase(), email: null, auth_method: null, status: 'ready', is_default: false, ...extra,
})
const plan = (percent, extra = {}) => ({
  source: 'plan', percent, period: 'week', is_warning: false, is_over: false, ...extra,
})
const rowsOf = (input) => accountRows({
  claude: [], claudeBudgets: {}, codex: [], codexBudgets: {}, sharedCodexLabel: 'Shared Codex login', sharedCodexUsage: null, ...input,
})

test('Claude rows first, then Codex, each keyed by kind', () => {
  const rows = rowsOf({ claude: [acc('a')], codex: [acc('a')] })
  assert.deepEqual(rows.map((r) => r.key), [accountKey('claude', 'a'), SHARED_CODEX_KEY, accountKey('codex', 'a')])
  assert.deepEqual(rows.map((r) => r.kind), ['claude', 'codex', 'codex'])
})

test('no Codex account: no shared-login row either', () => {
  const rows = rowsOf({ claude: [acc('a')], sharedCodexUsage: plan(40) })
  assert.deepEqual(rows.map((r) => r.key), ['claude:a'])
})

test('the shared Codex login is the default until a Codex account is', () => {
  let shared = rowsOf({ codex: [acc('x')] }).find((r) => r.key === SHARED_CODEX_KEY)
  assert.equal(shared.isDefault, true)
  assert.equal(shared.id, null)
  assert.equal(shared.detail, '~/.codex')
  shared = rowsOf({ codex: [acc('x', { is_default: true })] }).find((r) => r.key === SHARED_CODEX_KEY)
  assert.equal(shared.isDefault, false)
})

test('usage only from a plan verdict, per login of either kind', () => {
  const rows = rowsOf({
    claude: [acc('a'), acc('b')],
    claudeBudgets: { a: plan(72, { is_warning: true }), b: { ...plan(10), source: 'disabled' } },
    codex: [acc('x'), acc('y')],
    codexBudgets: { y: plan(91, { is_over: true }) },
    sharedCodexUsage: plan(15, { period: '5h' }),
  })
  const by = Object.fromEntries(rows.map((r) => [r.key, r.usage]))
  assert.deepEqual(by['claude:a'], { percent: 72, period: 'week', warning: true, over: false })
  assert.equal(by['claude:b'], null)
  assert.deepEqual(by[SHARED_CODEX_KEY], { percent: 15, period: '5h', warning: false, over: false })
  assert.equal(by['codex:x'], null)
  assert.deepEqual(by['codex:y'], { percent: 91, period: 'week', warning: false, over: true })
})

test('detail: the e-mail, else the sign-in method', () => {
  const rows = rowsOf({ claude: [acc('a', { email: 'a@example.com', auth_method: 'oauth' }), acc('b', { auth_method: 'oauth' })] })
  assert.deepEqual(rows.map((r) => r.detail), ['a@example.com', 'oauth'])
})

test('browser sign-in: the page step is done once the link is known', () => {
  assert.deepEqual(loginSteps({ url: null, code: null, deviceAuth: false }), [
    { key: 'open', state: 'active' }, { key: 'wait', state: 'active' },
  ])
  assert.deepEqual(loginSteps({ url: 'https://x', code: null, deviceAuth: false }), [
    { key: 'open', state: 'done' }, { key: 'wait', state: 'active' },
  ])
})

test('device-code sign-in: the wait starts once the code is there', () => {
  assert.deepEqual(loginSteps({ url: 'https://x', code: null, deviceAuth: true }), [
    { key: 'open', state: 'done' }, { key: 'code', state: 'active' }, { key: 'wait', state: 'pending' },
  ])
  assert.deepEqual(loginSteps({ url: 'https://x', code: 'ABCD-1234', deviceAuth: true }), [
    { key: 'open', state: 'done' }, { key: 'code', state: 'done' }, { key: 'wait', state: 'active' },
  ])
})
