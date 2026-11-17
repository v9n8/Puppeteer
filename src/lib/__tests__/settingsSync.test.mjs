// Unit tests for lib/settingsSync.ts (F-R6-3: pickers of other windows follow a save in Settings).
// Run: node --test src/lib/__tests__/settingsSync.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { SETTINGS_CHANGED, startSettingsSync, notifySettingsChanged } from '../settingsSync.ts'

test('an event from another window triggers a refresh', async () => {
  let handler
  let refreshed = 0
  await startSettingsSync(async (name, h) => { assert.equal(name, SETTINGS_CHANGED); handler = h }, async () => { refreshed++ })
  handler({ payload: { keys: ['enabled_models'] } })
  assert.equal(refreshed, 1)
})

test('a failing refresh does not throw out of the listener', async () => {
  let handler
  const warn = console.warn
  const seen = []
  console.warn = (...a) => seen.push(a)
  try {
    await startSettingsSync(async (_n, h) => { handler = h }, async () => { throw new Error('boom') })
    handler({ payload: { keys: [] } })
    await new Promise((r) => setTimeout(r, 0))
  } finally {
    console.warn = warn
  }
  assert.equal(seen.length, 1)
})

test('notify emits the changed keys', async () => {
  const sent = []
  await notifySettingsChanged(async (n, p) => { sent.push([n, p]) }, ['enabled_models'])
  assert.deepEqual(sent, [[SETTINGS_CHANGED, { keys: ['enabled_models'] }]])
})

test('a failing emit is logged, not thrown', async () => {
  const warn = console.warn
  let n = 0
  console.warn = () => { n++ }
  try {
    await notifySettingsChanged(async () => { throw new Error('x') }, ['a'])
  } finally {
    console.warn = warn
  }
  assert.equal(n, 1)
})
