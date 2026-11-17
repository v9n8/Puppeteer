// Run: node --test src/composables/__tests__/useConfirm.test.mjs
// DoD i18n: the composable carries no English default; the host translates blanks.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { useConfirm } from '../../composables/useConfirm.ts'

test('DoD i18n: a confirm without labels leaves them blank for the host to translate', () => {
  const { state, confirm, respond } = useConfirm()
  const pending = confirm('Ẩn flag này?')
  assert.equal(state.cancelLabel, '')
  assert.equal(state.confirmLabel, '')
  assert.equal(state.title, '')
  respond(null)
  return pending
})

test('explicit labels are kept', () => {
  const { state, confirm, respond } = useConfirm()
  const pending = confirm({ message: 'x', cancelLabel: 'Hủy', confirmLabel: 'Xóa' })
  assert.equal(state.cancelLabel, 'Hủy')
  assert.equal(state.confirmLabel, 'Xóa')
  respond(null)
  return pending
})
