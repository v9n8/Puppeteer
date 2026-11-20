// RT-22 (#8) on the remote controller: the Host pushes a run's file list
// (`project_file_list`), and the controller kept one list that every push and
// every run switch replaced. It is now cached by project path: switching back
// to a project shows its list at once, and a push that brings the same list
// keeps the array (and version) as they were.
// Run: node --test src/lib/__tests__/controllerProjectFiles.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { putFileList } from '../projectFileCache.ts'
import { createPushedFileLists, fileListKey } from '../../controller/projectFileLists.ts'

const runs = [
  { id: 'r1', project_id: 'pa' },
  { id: 'r2', project_id: 'pb' },
  { id: 'r3', project_id: 'pa' },
]
const projects = [
  { id: 'pa', path: '/p/a' },
  { id: 'pb', path: '/p/b' },
]

test('putFileList moves the version only when the list changed', () => {
  const lists = {}
  assert.equal(putFileList(lists, '/p/a', [{ path: 'a.ts', is_dir: false }]), true)
  const v = lists['/p/a'].version
  const entries = lists['/p/a'].entries
  assert.equal(putFileList(lists, '/p/a', [{ path: 'a.ts', is_dir: false }]), false)
  assert.equal(lists['/p/a'].version, v)
  assert.equal(lists['/p/a'].entries, entries)
  assert.equal(putFileList(lists, '/p/b', [{ path: 'b.ts', is_dir: false }]), true)
  assert.notEqual(lists['/p/b'].version, v, 'two paths never share a version')
  assert.equal(putFileList(lists, '/p/a', [{ path: 'a.ts', is_dir: false }, { path: 'n.ts', is_dir: false }]), true)
  assert.ok(lists['/p/a'].version > lists['/p/b'].version)
})

test('fileListKey is the project path, or the run before the lists arrive', () => {
  assert.equal(fileListKey('r1', runs, projects), '/p/a')
  assert.equal(fileListKey('r3', runs, projects), '/p/a', 'runs of one project share a list')
  assert.equal(fileListKey('r1', [], []), 'run:r1')
  assert.equal(fileListKey('r9', runs, projects), 'run:r9')
})

test('switching back to a project reuses its list; the same push keeps it', () => {
  const c = createPushedFileLists()
  const a = ['a.ts', 'src/x.ts']
  assert.equal(c.put('/p/a', a), true)
  assert.equal(c.put('/p/b', ['b.ts']), true)
  assert.equal(c.get('/p/a'), a)
  const v = c.version('/p/a')
  // The Host walks again on the next bind and pushes the same list.
  assert.equal(c.put('/p/a', ['a.ts', 'src/x.ts']), false)
  assert.equal(c.get('/p/a'), a, 'same array, nothing to recompute')
  assert.equal(c.version('/p/a'), v)
  // A new file: the list and version move.
  const next = ['a.ts', 'src/x.ts', 'new.ts']
  assert.equal(c.put('/p/a', next), true)
  assert.equal(c.get('/p/a'), next)
  assert.ok(c.version('/p/a') > v)
  c.clear()
  assert.equal(c.get('/p/a'), undefined)
  assert.equal(c.version('/p/a'), 0)
})
