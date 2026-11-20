// RT-22 (#8): the project file list is cached by project path, so switching
// project (which remounts RunView) does not walk the folder again, and the
// version the log's v-memo keys on only moves when the list really changed.
// Run: node --test src/lib/__tests__/projectFileCache.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { createProjectFileCache, sameFileList } from '../projectFileCache.ts'

const f = (path, is_dir = false) => ({ path, is_dir })

function fakeLoader(trees) {
  const calls = []
  const load = async (path) => {
    calls.push(path)
    const t = trees[path]
    if (t instanceof Error) throw t
    return t.map((e) => ({ ...e }))
  }
  return { load, calls }
}

test('sameFileList compares path, kind and order', () => {
  assert.equal(sameFileList([f('a'), f('b', true)], [f('a'), f('b', true)]), true)
  assert.equal(sameFileList([f('a')], [f('a'), f('b')]), false)
  assert.equal(sameFileList([f('a')], [f('a', true)]), false)
  assert.equal(sameFileList([f('a'), f('b')], [f('b'), f('a')]), false)
  assert.equal(sameFileList([], []), true)
})

test('switching between two projects twice walks each folder once', async () => {
  const { load, calls } = fakeLoader({ '/p/a': [f('a.ts')], '/p/b': [f('b.ts')] })
  const lists = {}
  const cache = createProjectFileCache(lists, load)
  // Each "mount" of RunView calls ensure() without force (useRunSession).
  for (const p of ['/p/a', '/p/b', '/p/a', '/p/b', '/p/a']) await cache.ensure(p)
  assert.deepEqual(calls, ['/p/a', '/p/b'])
  assert.deepEqual(lists['/p/a'].entries, [f('a.ts')])
  assert.notEqual(lists['/p/a'].version, lists['/p/b'].version)
})

test('the version does not move when coming back to a project', async () => {
  const { load } = fakeLoader({ '/p/a': [f('a.ts')], '/p/b': [f('b.ts')] })
  const lists = {}
  const cache = createProjectFileCache(lists, load)
  await cache.ensure('/p/a')
  const v = lists['/p/a'].version
  const entries = lists['/p/a'].entries
  await cache.ensure('/p/b')
  assert.equal(await cache.ensure('/p/a'), false)
  assert.equal(lists['/p/a'].version, v)
  assert.equal(lists['/p/a'].entries, entries, 'same array, nothing to re-render')
})

test('a forced reload bumps the version only when the list changed', async () => {
  const trees = { '/p/a': [f('a.ts')] }
  const { load, calls } = fakeLoader(trees)
  const lists = {}
  const cache = createProjectFileCache(lists, load)
  await cache.ensure('/p/a')
  const v = lists['/p/a'].version
  assert.equal(await cache.ensure('/p/a', true), false)
  assert.equal(lists['/p/a'].version, v)
  trees['/p/a'] = [f('a.ts'), f('new.ts')]
  assert.equal(await cache.ensure('/p/a', true), true)
  assert.ok(lists['/p/a'].version > v)
  assert.deepEqual(lists['/p/a'].entries.map((e) => e.path), ['a.ts', 'new.ts'])
  assert.equal(calls.length, 3)
})

test('concurrent calls for one path share one walk', async () => {
  const { load, calls } = fakeLoader({ '/p/a': [f('a.ts')] })
  const cache = createProjectFileCache({}, load)
  await Promise.all([cache.ensure('/p/a'), cache.ensure('/p/a', true), cache.ensure('/p/a')])
  assert.equal(calls.length, 1)
})

test('a failed walk keeps the cached list, and an uncached path tries again', async () => {
  const trees = { '/p/a': [f('a.ts')], '/p/x': new Error('nope') }
  const { load, calls } = fakeLoader(trees)
  const lists = {}
  const cache = createProjectFileCache(lists, load)
  await cache.ensure('/p/a')
  const v = lists['/p/a'].version
  trees['/p/a'] = new Error('io')
  assert.equal(await cache.ensure('/p/a', true), false)
  assert.equal(lists['/p/a'].version, v)
  assert.deepEqual(lists['/p/a'].entries, [f('a.ts')])
  await cache.ensure('/p/x')
  assert.equal(cache.has('/p/x'), false)
  await cache.ensure('/p/x')
  assert.equal(calls.filter((p) => p === '/p/x').length, 2)
})

test('an empty project is cached too (no walk on every mount)', async () => {
  const { load, calls } = fakeLoader({ '/p/e': [] })
  const cache = createProjectFileCache({}, load)
  await cache.ensure('/p/e')
  await cache.ensure('/p/e')
  assert.equal(calls.length, 1)
})
