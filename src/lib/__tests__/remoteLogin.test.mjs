// Unit tests for lib/remoteLogin.ts — must match src-tauri/src/remote/login.rs.
// Run: node --test src/lib/__tests__/remoteLogin.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  DEVICE_ALIAS_RE,
  LOGIN_LINK_SECRET,
  loginCodeFor,
  relayWsUrl,
  sameOriginRelayUrl,
  stretchPassword,
} from '../remoteLogin.ts'
import { deriveEffectivePsk, ready } from '../../remote/e2e/index.ts'

const hex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('')

test('stretchPassword is PBKDF2-HMAC-SHA256 with the shared salt and rounds', async () => {
  assert.equal(
    await stretchPassword('puppeteer-test-password'),
    '4742cc3995acb3e89da012f58575f21c7d356d2d6b441cde621207bc570d8877',
  )
})

test('login PSK matches the Rust host vector', async () => {
  await ready()
  const psk = deriveEffectivePsk(LOGIN_LINK_SECRET, await stretchPassword('puppeteer-test-password'))
  assert.equal(hex(psk), '95db3565ad5fad0b134b257f669773234d2750ebe9ed3350bc4c5f5715f2286b')
})

test('sameOriginRelayUrl follows the page scheme', () => {
  assert.equal(sameOriginRelayUrl({ protocol: 'https:', host: 'relay.example.com' }), 'wss://relay.example.com/ws')
  assert.equal(sameOriginRelayUrl({ protocol: 'http:', host: 'localhost:8080' }), 'ws://localhost:8080/ws')
})

test('loginCodeFor mirrors login::login_code', () => {
  assert.equal(loginCodeFor(), 'puppeteer-login-v1')
  assert.equal(loginCodeFor(''), 'puppeteer-login-v1')
  assert.equal(loginCodeFor('  '), 'puppeteer-login-v1')
  assert.equal(loginCodeFor('mac-mini'), 'puppeteer-login-v1/mac-mini')
})

test('DEVICE_ALIAS_RE mirrors login::valid_alias', () => {
  assert.ok(DEVICE_ALIAS_RE.test(''))
  assert.ok(DEVICE_ALIAS_RE.test('mac-mini-2'))
  assert.ok(DEVICE_ALIAS_RE.test('a'.repeat(32)))
  assert.ok(!DEVICE_ALIAS_RE.test('a'.repeat(33)))
  assert.ok(!DEVICE_ALIAS_RE.test('Mac'))
  assert.ok(!DEVICE_ALIAS_RE.test('mac/mini'))
})

test('relayWsUrl keeps ws(s) URLs and maps http(s) to the /ws endpoint', () => {
  assert.equal(relayWsUrl('wss://relay.example.com/ws'), 'wss://relay.example.com/ws')
  assert.equal(relayWsUrl('ws://localhost:8080/ws'), 'ws://localhost:8080/ws')
  assert.equal(relayWsUrl('https://relay.example.com'), 'wss://relay.example.com/ws')
  assert.equal(relayWsUrl('http://localhost:8080/'), 'ws://localhost:8080/ws')
  assert.equal(relayWsUrl(''), null)
  assert.equal(relayWsUrl('not a url'), null)
})
