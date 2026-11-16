// Unit tests for lib/diffSplit.ts (V3-47).
// Run: node --test src/lib/__tests__/diffSplit.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { computeDiff } from '../diff.ts'
import { splitDiffRows } from '../diffSplit.ts'

const split = (a, b, opts) => splitDiffRows(computeDiff(a, b, opts).rows)
const texts = (rows) => rows.map((r) => (r.type === 'fold' ? `fold:${r.count}` : [r.left?.text ?? null, r.right?.text ?? null]))

test('only added lines: left stays empty, right numbered from 1', () => {
  const rows = split('', 'a\nb')
  assert.deepEqual(texts(rows), [[null, 'a'], [null, 'b']])
  assert.deepEqual(rows.map((r) => r.right.no), [1, 2])
})

test('only removed lines: right stays empty', () => {
  assert.deepEqual(texts(split('a\nb', '')), [['a', null], ['b', null]])
})

test('one changed line becomes one row with both sides', () => {
  const rows = split('a\nb\nc', 'a\nX\nc')
  assert.deepEqual(texts(rows), [['a', 'a'], ['b', 'X'], ['c', 'c']])
  assert.equal(rows[1].left.type, 'del')
  assert.equal(rows[1].right.type, 'add')
  assert.deepEqual([rows[1].left.no, rows[1].right.no], [2, 2])
})

test('unequal runs pad the short side', () => {
  assert.deepEqual(texts(split('a\nb1\nb2\nc', 'a\nX\nc')), [['a', 'a'], ['b1', 'X'], ['b2', null], ['c', 'c']])
})

test('long unchanged runs stay folded', () => {
  const body = Array.from({ length: 20 }, (_, i) => `l${i}`)
  const changed = [...body]
  changed[10] = 'changed'
  const rows = split(body.join('\n'), changed.join('\n'), { context: 2 })
  assert.deepEqual(rows[0], { type: 'fold', count: 8 })
  assert.equal(rows.at(-1).type, 'fold')
})

test('identical text has no change rows', () => {
  const rows = split('a\nb', 'a\nb')
  assert.ok(rows.every((r) => r.type === 'fold' || (r.left.type === 'ctx' && r.right.type === 'ctx')))
})
