// Unit tests for lib/windowThumbs.ts: which windows may be shot, what a card shows
// and when the "could not capture" banner appears.
// Run: node --test src/lib/__tests__/windowThumbs.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { isSnapshotAllowed, shouldShowBanner, thumbFor } from '../windowThumbs.ts'

test('isSnapshotAllowed never allows the vault, on this machine or a remote one', () => {
  assert.equal(isSnapshotAllowed('secret-notes'), false)
  assert.equal(isSnapshotAllowed('remote-secret-notes'), false)
  assert.equal(isSnapshotAllowed('main'), false)
  assert.equal(isSnapshotAllowed('remote-bridge'), false)
  assert.equal(isSnapshotAllowed('permission-prompt'), false)
  assert.equal(isSnapshotAllowed('remote-permission-x'), false)
  assert.equal(isSnapshotAllowed('calendar'), true)
  assert.equal(isSnapshotAllowed('session-1'), true)
  assert.equal(isSnapshotAllowed('remote-desktop'), true)
})

test('thumbFor: the vault is an icon even with a picture in the cache', () => {
  const cache = { 'secret-notes': { src: 'data:x', at: 1 } }
  assert.deepEqual(thumbFor({ label: 'secret-notes' }, cache, 'loading'), { kind: 'icon' })
})

test('thumbFor: picture, loading, icon', () => {
  const cache = { calendar: { src: 'data:cal', at: 1 } }
  assert.deepEqual(thumbFor({ label: 'calendar' }, cache, undefined), { kind: 'image', src: 'data:cal' })
  // A new shot is on its way: the old picture stays (no blink).
  assert.deepEqual(thumbFor({ label: 'calendar' }, cache, 'loading'), { kind: 'image', src: 'data:cal' })
  assert.deepEqual(thumbFor({ label: 'gantt' }, cache, 'loading'), { kind: 'loading' })
  assert.deepEqual(thumbFor({ label: 'gantt' }, cache, 'failed'), { kind: 'icon' })
  assert.deepEqual(thumbFor({ label: 'gantt' }, cache, undefined), { kind: 'icon' })
})

test('shouldShowBanner: only when every tried window failed', () => {
  assert.equal(shouldShowBanner([]), false)
  assert.equal(shouldShowBanner([{ label: 'a', error: 'timeout' }, { label: 'b', error: 'x' }]), true)
  assert.equal(shouldShowBanner([{ label: 'a', image: 'd' }, { label: 'b', error: 'x' }]), false)
  assert.equal(shouldShowBanner([{ label: 'a', error: 'unsupported' }]), false)
  assert.equal(shouldShowBanner([{ label: 'a', error: 'unsupported' }, { label: 'b', error: 'x' }]), true)
})
