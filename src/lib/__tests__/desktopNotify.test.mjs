// Unit tests for lib/desktopNotify.ts (desktop OS notification switches: all on
// until turned off). Run: node --test src/lib/__tests__/desktopNotify.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { DESKTOP_NOTIFY_KINDS, allows, parseDesktopNotify } from '../desktopNotify.ts'

const ALL_ON = { needs_you: true, done: true, failed: true, stuck: true, schedule: true, sound: true }

test('a profile without the key notifies about everything', () => {
  assert.deepEqual(parseDesktopNotify(''), ALL_ON)
  assert.deepEqual(parseDesktopNotify(undefined), ALL_ON)
  assert.deepEqual(parseDesktopNotify(null), ALL_ON)
})

test('broken or non-object JSON reads as all on, without throwing', () => {
  for (const raw of ['{', 'not json', 'null', '[]', '[false]', '"done"', '42', 'false']) {
    assert.deepEqual(parseDesktopNotify(raw), ALL_ON, raw)
  }
})

test('a missing field reads as on', () => {
  assert.deepEqual(parseDesktopNotify('{"done":false}'), { ...ALL_ON, done: false })
  assert.deepEqual(parseDesktopNotify('{}'), ALL_ON)
})

test('only an explicit false turns a kind off; unknown fields are ignored', () => {
  assert.deepEqual(
    parseDesktopNotify('{"stuck":0,"schedule":"false","sound":null,"failed":true,"later_kind":false}'),
    ALL_ON,
  )
})

test('turning one switch off leaves the others on', () => {
  const settings = parseDesktopNotify(JSON.stringify({ ...ALL_ON, stuck: false }))
  for (const k of DESKTOP_NOTIFY_KINDS) assert.equal(allows(settings, k), k !== 'stuck', k)
})

test('the six switches, in screen order', () => {
  assert.deepEqual([...DESKTOP_NOTIFY_KINDS], ['needs_you', 'done', 'failed', 'stuck', 'schedule', 'sound'])
})
