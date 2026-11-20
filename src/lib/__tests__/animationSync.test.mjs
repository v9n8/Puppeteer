// RT-28: looping loading animations (spinners, pings, the "thinking" marks) keep
// one beat across every session and window: their phase follows the wall clock,
// not the moment each element mounted.
// Run: node --test src/lib/__tests__/animationSync.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { animationPhase, shouldSync, syncedStartTime } from '../animationSync.ts'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')

test('shouldSync: the looping loading animations only', () => {
  for (const n of ['spin', 'ping', 'pulse', 'bounce', 'skeleton-sweep', 'thinking-bounce', 'thinking-shimmer-6f1a2b3c']) {
    assert.equal(shouldSync(n), true, n)
  }
  // One-shot entrances keep their own start.
  for (const n of ['fade-rise', 'prompt-in', 'spinner-x', '']) assert.equal(shouldSync(n), false, n)
})

test('two windows started apart show the same phase at the same instant', () => {
  // Window A opened at epoch 1_000_000.3 ms, window B 2.75 s later; each timeline
  // counts from its own open (performance.timeOrigin).
  const originA = 1_000_000.3
  const originB = originA + 2750
  const epoch = originA + 12_345.6
  const startA = syncedStartTime(originA)
  const startB = syncedStartTime(originB)
  // An animation's local time is its timeline's time minus its start time.
  const localA = (epoch - originA) - startA
  const localB = (epoch - originB) - startB
  assert.ok(Math.abs(animationPhase(localA, 1000) - animationPhase(localB, 1000)) < 1e-6)
  assert.ok(Math.abs(animationPhase(localA, 1400, 200) - animationPhase(localB, 1400, 200)) < 1e-6)
})

test('two elements of one window mounted apart spin in step', () => {
  const origin = 5_000
  // The start time does not depend on when the element mounted.
  assert.equal(syncedStartTime(origin), syncedStartTime(origin))
  const local = 30_000 - syncedStartTime(origin)
  assert.equal(animationPhase(local, 1000), (origin + 30_000) % 1000)
})

test('animationPhase: the stagger delay of a dot is kept, and wraps into [0, duration)', () => {
  assert.equal(animationPhase(1000, 1400, 200), 800)
  assert.equal(animationPhase(100, 1400, 200), 1300)
  assert.equal(animationPhase(0, 0), 0)
})

test('the sync is installed in the app and controller entries', () => {
  assert.match(read('../../main.ts'), /installAnimationSync\(\)/)
  assert.match(read('../../controller/main.ts'), /installAnimationSync\(\)/)
})
