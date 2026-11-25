// Every reason code the broker can send has a text in en and vi, and the broker
// prompt's texts use the same placeholders in both languages.
// Run: node --test src/lib/__tests__/brokerI18n.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8')
const messages = (lang) => JSON.parse(read(`../../i18n/locales/${lang}/permission.json`))
const BROKER = '../../../src-tauri/src/runs/broker/'

/** Codes the Rust side writes: the policy texts' table and its suffix parts, and the `api` causes. */
function rustCodes() {
  const reason = read(`${BROKER}approver/reason.rs`).split('#[cfg(test)]')[0]
  const apiCall = read(`${BROKER}policy/api_call.rs`)
  const codeFn = apiCall.slice(apiCall.indexOf('fn code('), apiCall.indexOf('fn params('))
  const codes = [
    ...[...reason.matchAll(/\("[^"]*", "(\w+)"\)/g)].map((m) => m[1]),
    ...[...reason.matchAll(/part\("(\w+)"/g)].map((m) => m[1]),
    ...[...codeFn.matchAll(/=> "(\w+)"/g)].map((m) => m[1]),
  ]
  return [...new Set(codes)]
}

test('the Rust sources yield the codes (the scan itself works)', () => {
  const codes = rustCodes()
  for (const c of ['gitPush', 'azWrite', 'asAccount', 'azSubscription', 'tfExternalCode', 'apiUnknownFlag', 'apiBackdoor']) {
    assert.ok(codes.includes(c), c)
  }
})

test('every broker reason code has a text in en and vi', () => {
  for (const lang of ['en', 'vi']) {
    const texts = messages(lang).brokerReason
    for (const code of rustCodes()) assert.equal(typeof texts[code], 'string', `${lang}: brokerReason.${code}`)
  }
})

const placeholders = (s) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort()

/** The broker prompt's texts as `[key, message]`; a group that does not exist yet adds nothing. */
function brokerTexts(lang) {
  const m = messages(lang)
  const group = (prefix, node) => Object.entries(node ?? {}).map(([k, v]) => [`${prefix}.${k}`, v])
  return [
    ...group('impact.action', m.impact.action),
    ...group('brokerReason', m.brokerReason),
    ...['hintPermissionOnce', 'allowShape', 'allowShapeTitle'].filter((k) => k in m).map((k) => [k, m[k]]),
  ]
}

test('en and vi have the same broker keys with the same placeholders', () => {
  const en = Object.fromEntries(brokerTexts('en'))
  const vi = Object.fromEntries(brokerTexts('vi'))
  assert.deepEqual(Object.keys(vi).sort(), Object.keys(en).sort())
  for (const key of Object.keys(en)) assert.deepEqual(placeholders(vi[key]), placeholders(en[key]), key)
})
