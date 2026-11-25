// Unit tests for lib/fileWindowCore.ts (the file pop-out's URL query).
// Run: node --test src/lib/__tests__/fileWindowCore.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { fileWindowQuery, parseFileWindowQuery } from '../fileWindowCore.ts'

test('fileWindowQuery: flag, project and path; no line, origin or root when absent', () => {
  const q = new URLSearchParams(fileWindowQuery({ projectPath: '/p', root: '/p', path: 'src/a.ts', line: null, originRunId: null }))
  assert.equal(q.get('fileWindow'), '1')
  assert.equal(q.get('projectPath'), '/p')
  assert.equal(q.get('path'), 'src/a.ts')
  assert.equal(q.has('line'), false)
  assert.equal(q.has('originRunId'), false)
  assert.equal(q.has('root'), false)
})

test('fileWindowQuery → parseFileWindowQuery round-trips line and origin run', () => {
  const p = { projectPath: '/my proj', root: '/my proj', path: '/my proj/a&b?.md', line: 42, originRunId: 'run-1' }
  assert.deepEqual(parseFileWindowQuery(`?${fileWindowQuery(p)}`), p)
})

test('fileWindowQuery: line 0 is kept', () => {
  assert.equal(parseFileWindowQuery(fileWindowQuery({ projectPath: '/p', root: '/p', path: 'a', line: 0, originRunId: null })).line, 0)
})

test('parseFileWindowQuery: an old URL without origin or root reads as no origin, read from the project', () => {
  assert.deepEqual(parseFileWindowQuery('?fileWindow=1&projectPath=%2Fp&path=a.ts'), {
    projectPath: '/p',
    root: '/p',
    path: 'a.ts',
    line: null,
    originRunId: null,
  })
})

test('a worker worktree root travels beside the project', () => {
  const p = { projectPath: '/p', root: '/wt/r 1', path: 'tasks/x.md', line: null, originRunId: 'run-1' }
  const q = fileWindowQuery(p)
  assert.equal(new URLSearchParams(q).get('root'), '/wt/r 1')
  assert.deepEqual(parseFileWindowQuery(`?${q}`), p)
})
