// RT-07: a live session that does not hold its log from the start (seeded
// from a history window, or opened mid-run in another window) pages in the
// older part from disk like the history pane.
// Run: node --test src/lib/__tests__/livePaging.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { register } from 'node:module'

register(
  'data:text/javascript,' +
    encodeURIComponent(`
      export async function resolve(spec, ctx, next) {
        try { return await next(spec, ctx) } catch (e) {
          if (spec.startsWith('.') && !/\\.[cm]?[jt]s$/.test(spec)) return next(spec + '.ts', ctx)
          throw e
        }
      }`),
)
const { mergeTailWithLive, pagingFromHistory, prependPage, COMPLETE_PAGING, toolIndexOf } = await import('../livePaging.ts')

const user = (text, extra = {}) => ({ kind: 'user', text, ...extra })
const tool = (id) => ({ kind: 'tool', id, name: 'Bash', input: {} })
const text = (t) => ({ kind: 'text', text: t })

test('a live copy opened mid-run takes the disk tail before its first prompt or tool', () => {
  const disk = [user('one'), text('a'), tool('t1'), text('b'), user('two'), tool('t2')]
  // Listening started mid-text: a fragment, then the tool the disk also has.
  const live = [text('b-frag'), user('two'), tool('t2'), text('streaming')]
  const m = mergeTailWithLive(disk, live)
  assert.equal(m.kind, 'merged')
  assert.deepEqual(m.entries, [user('one'), text('a'), tool('t1'), text('b'), user('two'), tool('t2'), text('streaming')])
})

test('the live entry is kept over the disk copy (it may still be pending its echo)', () => {
  const mine = user('two', { echoPending: true })
  const m = mergeTailWithLive([user('one'), user('two')], [mine])
  assert.equal(m.entries[1], mine)
})

test('a repeated prompt aligns on the run of anchors, not the last match', () => {
  const disk = [user('go'), tool('a'), user('go'), tool('b'), user('go'), tool('c')]
  const live = [user('go'), tool('b'), user('go'), tool('c')]
  const m = mergeTailWithLive(disk, live)
  assert.deepEqual(m.entries.map((e) => e.id ?? e.text), ['go', 'a', 'go', 'b', 'go', 'c'])
})

test('the disk tail is all inside the live copy: ask for an older page', () => {
  const live = [user('one'), tool('t1'), user('two'), tool('t2')]
  assert.deepEqual(mergeTailWithLive([user('two'), tool('t2')], live), { kind: 'older' })
})

test('the disk ends before the live copy starts: both, disk first', () => {
  const m = mergeTailWithLive([user('one'), tool('t1')], [user('two'), tool('t2')])
  assert.deepEqual(m.entries, [user('one'), tool('t1'), user('two'), tool('t2')])
})

test('nothing on disk yet: the live copy as it is', () => {
  const live = [user('one')]
  assert.deepEqual(mergeTailWithLive([], live).entries, live)
})

test('a seed from a history window keeps its cursor; a mid-file window cannot seed', () => {
  assert.deepEqual(pagingFromHistory({ cursor: 1200, hasMore: true, hasMoreBelow: false, seedModel: 'm' }), { cursor: 1200, hasMore: true, seedModel: 'm' })
  assert.equal(pagingFromHistory({ cursor: 1200, hasMore: true, hasMoreBelow: true, seedModel: null }), null)
  assert.equal(pagingFromHistory({ cursor: null, hasMore: false, hasMoreBelow: false, seedModel: null }).hasMore, false)
  assert.equal(COMPLETE_PAGING.hasMore, false)
})

test('an older page goes in front, oldest first; the tool index follows', () => {
  const entries = prependPage([user('two'), tool('t2')], [user('one'), tool('t1')])
  assert.deepEqual(entries.map((e) => e.id ?? e.text), ['one', 't1', 'two', 't2'])
  assert.deepEqual([...toolIndexOf(entries)], [['t1', 1], ['t2', 3]])
})
