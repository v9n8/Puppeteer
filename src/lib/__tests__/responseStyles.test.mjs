// Unit tests for lib/responseStyles.ts (UX-17).
// Run: node --test src/lib/__tests__/responseStyles.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  BUILTIN_STYLE_KEYS, STYLE_BODY_MAX, isBuiltinStyle, styleFormError, uniqueStyleName,
} from '../responseStyles.ts'

test('built-in keys match the backend list', () => {
  assert.deepEqual([...BUILTIN_STYLE_KEYS], ['default', 'concise', 'explain_junior', 'code_review'])
  assert.equal(isBuiltinStyle('concise'), true)
  assert.equal(isBuiltinStyle('3f2a-uuid'), false)
  assert.equal(isBuiltinStyle(''), false)
  assert.equal(isBuiltinStyle(null), false)
})

test('a copied name gets a free suffix, case-insensitively', () => {
  assert.equal(uniqueStyleName('Concise', []), 'Concise')
  assert.equal(uniqueStyleName(' Concise ', ['other']), 'Concise')
  assert.equal(uniqueStyleName('Concise', ['concise']), 'Concise (2)')
  assert.equal(uniqueStyleName('Concise', ['Concise', 'CONCISE (2)']), 'Concise (3)')
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
