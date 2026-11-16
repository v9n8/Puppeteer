// Unit tests for lib/projectRunDots.ts (Project list dots and middle-cut path, V3-45).
// Run: node --test src/lib/__tests__/projectRunDots.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { projectRunDots, middlePath } from '../projectRunDots.ts'

const s = (projectId, status, pending = 0) => ({ projectId, status, permissionQueue: new Array(pending) })

test('no live session gives no dot', () => {
  assert.deepEqual(projectRunDots([]), {})
  assert.deepEqual(projectRunDots([s('a', 'done')]), {})
})

test('running session gives a running dot', () => {
  assert.deepEqual(projectRunDots([s('a', 'running')]), { a: 'running' })
})

test('a waiting permission gives a waiting dot', () => {
  assert.deepEqual(projectRunDots([s('b', 'running', 1)]), { b: 'waiting' })
})

test('waiting beats running whatever the order', () => {
  assert.deepEqual(projectRunDots([s('a', 'running'), s('a', 'running', 2)]), { a: 'waiting' })
  assert.deepEqual(projectRunDots([s('a', 'running', 2), s('a', 'running')]), { a: 'waiting' })
})

test('short path stays whole', () => {
  assert.deepEqual(middlePath('~/repos/app', 30), { head: '~/repos/app', middle: '', tail: '' })
})

test('long path keeps the start and the last folder', () => {
  assert.deepEqual(middlePath('~/WorkSpace/repos/Personal/Puppeteer', 20), {
    head: '~/WorkSpace', middle: '/repos/Personal', tail: '/Puppeteer',
  })
})

test('absolute path keeps three parts of the head', () => {
  const r = middlePath('/Users/me/WorkSpace/repos/app', 10)
  assert.deepEqual(r, { head: '/Users/me', middle: '/WorkSpace/repos', tail: '/app' })
  assert.equal(r.head + r.middle + r.tail, '/Users/me/WorkSpace/repos/app')
})

test('long last folder name is left whole for CSS to cut', () => {
  const r = middlePath('~/a/b/' + 'x'.repeat(60), 20)
  assert.equal(r.tail, '/' + 'x'.repeat(60))
})

test('Windows path', () => {
  const r = middlePath('C:\\Users\\me\\repos\\Personal\\Puppeteer', 20)
  assert.deepEqual(r, { head: 'C:\\Users', middle: '\\me\\repos\\Personal', tail: '\\Puppeteer' })
})

test('too few folders to have a middle', () => {
  assert.equal(middlePath('~/' + 'x'.repeat(50), 20).middle, '')
})
