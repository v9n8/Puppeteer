// Unit tests for lib/responseStyles.ts (UX-17, UX-36).
// Run: node --test src/lib/__tests__/responseStyles.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  NO_STYLE, SAMPLE_STYLE_KEYS, STYLE_BODY_MAX, missingSamples, shownStyles, styleFormError, styleText,
} from '../responseStyles.ts'

test('the samples match the backend list', () => {
  assert.equal(NO_STYLE, 'default')
  assert.deepEqual([...SAMPLE_STYLE_KEYS], ['concise', 'explain_junior', 'code_review'])
})

const t = (key) => `<${key}>`
const sample = {
  id: 'concise', name: 'Concise', description: 'Answer first', builtin_key: 'concise', stock_name: true, stock_description: true,
}

test('a sample shows its translated texts until the user edits them', () => {
  assert.deepEqual(styleText(sample, t), {
    name: '<responseStyles.builtin.concise.name>',
    description: '<responseStyles.builtin.concise.description>',
  })
  assert.deepEqual(styleText({ ...sample, name: 'Short', stock_name: false }, t), {
    name: 'Short',
    description: '<responseStyles.builtin.concise.description>',
  })
  // The user's own style: always what they typed.
  const own = { id: 'u1', name: 'Mine', description: 'd', builtin_key: null, stock_name: false, stock_description: false }
  assert.deepEqual(styleText(own, t), { name: 'Mine', description: 'd' })
  assert.deepEqual(shownStyles([sample, own], t).map((s) => [s.id, s.name]), [
    ['concise', '<responseStyles.builtin.concise.name>'],
    ['u1', 'Mine'],
  ])
})

test('restore offers the samples no longer in the list', () => {
  assert.deepEqual(missingSamples([sample, { builtin_key: null }]), ['explain_junior', 'code_review'])
  assert.deepEqual(missingSamples(SAMPLE_STYLE_KEYS.map((k) => ({ builtin_key: k }))), [])
})

test('form validation mirrors the backend', () => {
  const ok = { name: 'Short', description: '', body: 'Be short.' }
  assert.equal(styleFormError(ok), null)
  assert.equal(styleFormError({ ...ok, name: '  ' }), 'responseStyles.errors.nameRequired')
  assert.equal(styleFormError({ ...ok, name: 'n'.repeat(81) }), 'responseStyles.errors.nameTooLong')
  assert.equal(styleFormError({ ...ok, description: 'd'.repeat(301) }), 'responseStyles.errors.descriptionTooLong')
  assert.equal(styleFormError({ ...ok, body: ' ' }), 'responseStyles.errors.bodyRequired')
  assert.equal(styleFormError({ ...ok, body: 'x'.repeat(STYLE_BODY_MAX + 1) }), 'responseStyles.errors.bodyTooLong')
})
