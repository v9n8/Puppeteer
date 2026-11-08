// Unit tests for lib/whatsNew.ts (the "What's new" popup, `app.whats_new`).
// Run: node --test src/lib/__tests__/whatsNew.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { RELEASES, compareVersions, noteText, whatsNewAtLaunch } from '../whatsNew.ts'

const rel = (version) => ({ version, date: '2026-11-08', items: [{ kind: 'added', en: `en ${version}`, vi: `vi ${version}` }] })
const versions = (r) => r.notes.map((n) => n.version)

test('compareVersions follows semver precedence', () => {
  assert.equal(compareVersions('0.13.1', '0.13.0'), 1)
  assert.equal(compareVersions('0.10.0', '0.9.0'), 1, 'numeric, not text')
  assert.equal(compareVersions('1.0.0', '0.99.99'), 1)
  assert.equal(compareVersions('0.13.0', '0.13.0'), 0)
  assert.equal(compareVersions('0.13.9', '0.14.0'), -1)
  assert.equal(compareVersions('0.13.0+build.7', '0.13.0'), 0, 'build metadata is not a release')
  assert.equal(compareVersions('0.14.0-rc.1', '0.14.0'), -1, 'a pre-release is older than its release')
  assert.equal(compareVersions('0.14.0-rc.1', '0.13.0'), 1)
  assert.equal(compareVersions('0.14.0-rc.2', '0.14.0-rc.10'), -1)
  assert.equal(compareVersions('0.14.0-1', '0.14.0-alpha'), -1, 'numeric ids rank below alphanumeric')
  assert.equal(compareVersions('0.14.0-alpha', '0.14.0-alpha.1'), -1)
  assert.equal(compareVersions('0.14', '0.13.0'), null)
  assert.equal(compareVersions('', '0.13.0'), null)
})

test('first launch with the feature records the version without a popup', () => {
  for (const seen of [null, undefined, '', 'garbage']) {
    const r = whatsNewAtLaunch('0.14.0', seen, [rel('0.14.0')])
    assert.deepEqual(r, { notes: [], markSeen: '0.14.0' }, String(seen))
  }
})

test('a newer version shows its notes once', () => {
  const releases = [rel('0.14.0'), rel('0.13.0')]
  const first = whatsNewAtLaunch('0.14.0', '0.13.0', releases)
  assert.deepEqual(versions(first), ['0.14.0'])
  assert.equal(first.markSeen, '0.14.0')
  // Next launch: the stored version is now the running one.
  assert.deepEqual(whatsNewAtLaunch('0.14.0', first.markSeen, releases), { notes: [], markSeen: null })
})

test('skipped versions are shown together, newest first, nothing past the running one', () => {
  const releases = [rel('0.13.1'), rel('0.15.0'), rel('0.14.0'), rel('0.13.0')]
  const r = whatsNewAtLaunch('0.14.0', '0.13.0', releases)
  assert.deepEqual(versions(r), ['0.14.0', '0.13.1'])
})

test('a version without notes moves the mark silently', () => {
  assert.deepEqual(whatsNewAtLaunch('0.14.1', '0.14.0', [rel('0.14.0')]), { notes: [], markSeen: '0.14.1' })
})

test('the mark never moves down: back to an older build and up again shows nothing twice', () => {
  const releases = [rel('0.14.0')]
  assert.deepEqual(whatsNewAtLaunch('0.13.0', '0.14.0', releases), { notes: [], markSeen: null })
  assert.deepEqual(whatsNewAtLaunch('0.14.0', '0.14.0', releases), { notes: [], markSeen: null })
})

test('an unreadable running version does nothing', () => {
  assert.deepEqual(whatsNewAtLaunch('', '0.13.0', [rel('0.14.0')]), { notes: [], markSeen: null })
})

test('noteText picks the UI language, English otherwise', () => {
  const note = { kind: 'fixed', en: 'Fixed', vi: 'Đã sửa' }
  assert.equal(noteText(note, 'vi'), 'Đã sửa')
  assert.equal(noteText(note, 'en'), 'Fixed')
  assert.equal(noteText(note, 'ja'), 'Fixed')
})

test('bundled notes: valid versions, newest first, both languages', () => {
  for (const [i, r] of RELEASES.entries()) {
    assert.notEqual(compareVersions(r.version, '0.0.0'), null, r.version)
    assert.match(r.date, /^\d{4}-\d{2}-\d{2}$/, r.version)
    assert.ok(r.items.length, r.version)
    for (const item of r.items) {
      assert.ok(['added', 'improved', 'changed', 'fixed'].includes(item.kind), r.version)
      assert.ok(item.en.trim() && item.vi.trim(), r.version)
    }
    if (i > 0) assert.equal(compareVersions(RELEASES[i - 1].version, r.version), 1, r.version)
  }
})
