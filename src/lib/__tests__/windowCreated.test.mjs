// Run: node --test src/lib/windowCreated.test.mjs
import test from 'node:test'
import assert from 'node:assert/strict'
import { whenWindowCreated } from '../windowCreated.ts'

const fakeWin = () => {
  const h = {}
  return { win: { once: (ev, fn) => { h[ev] = fn } }, fire: (ev, payload) => h[ev]({ payload }) }
}

test('resolves on tauri://created', async () => {
  const { win, fire } = fakeWin()
  const p = whenWindowCreated(win)
  fire('tauri://created')
  await p
})

test('rejects with the error payload on tauri://error', async () => {
  const { win, fire } = fakeWin()
  const p = whenWindowCreated(win)
  fire('tauri://error', 'no such label')
  await assert.rejects(p, /no such label/)
})
