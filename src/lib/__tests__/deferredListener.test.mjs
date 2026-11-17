// Run: node --test src/lib/__tests__/deferredListener.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { addKeydownNextTick } from '../deferredListener.ts'

function fakeTarget() {
  const fns = new Set()
  return {
    fns,
    addEventListener: (_t, fn) => fns.add(fn),
    removeEventListener: (_t, fn) => fns.delete(fn),
  }
}
const tick = () => new Promise((r) => setTimeout(r, 5))

test('AC-6 the key press that opens a dialog cannot reach its listener', async () => {
  const target = fakeTarget()
  addKeydownNextTick(target, () => {})
  assert.equal(target.fns.size, 0)
  await tick()
  assert.equal(target.fns.size, 1)
})

test('removing before the tick cancels the pending add', async () => {
  const target = fakeTarget()
  const remove = addKeydownNextTick(target, () => {})
  remove()
  await tick()
  assert.equal(target.fns.size, 0)
})

test('removing after the tick detaches the listener', async () => {
  const target = fakeTarget()
  const remove = addKeydownNextTick(target, () => {})
  await tick()
  remove()
  assert.equal(target.fns.size, 0)
})
