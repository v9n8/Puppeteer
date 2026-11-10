// Unit tests for lib/trash.ts (Trash screen grouping + sorting).
// Run: node --test src/lib/__tests__/
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { groupTrash, parseTrashSort, sessionsLost, trashTotals } from '../trash.ts'

function project(id, extra = {}) {
  return { id, name: id.toUpperCase(), path: `/${id}`, created_at: '2026-01-01T00:00:00Z', deleted_at: null, ...extra }
}

function run(id, project_id, extra = {}) {
  return {
    id,
    project_id,
    title: null,
    run_type: 'session',
    ref_number: null,
    status: 'done',
    engine: 'claude',
    model: null,
    created_at: '2026-02-01T00:00:00Z',
    deleted_at: null,
    conductor_run_id: null,
    worker_count: 0,
    ...extra,
  }
}

// `live`: a live project with two sessions trashed on their own, one of them a
// conductor trashed with a worker. `gone`: a trashed project with two sessions,
// one of them trashed on its own earlier.
const LISTING = {
  projects: [
    project('gone', { deleted_at: '2026-03-05T00:00:00Z' }),
    project('live'),
  ],
  runs: [
    run('a', 'live', { created_at: '2026-02-10T00:00:00Z', deleted_at: '2026-03-01T00:00:00Z' }),
    run('c', 'live', { created_at: '2026-02-01T00:00:00Z', deleted_at: '2026-03-03T00:00:00Z', conductor_run_id: 'c', worker_count: 2 }),
    run('w', 'live', { created_at: '2026-02-02T00:00:00Z', deleted_at: '2026-03-03T00:00:00Z', conductor_run_id: 'c' }),
    run('g1', 'gone', { created_at: '2026-02-20T00:00:00Z' }),
    run('g2', 'gone', { created_at: '2026-02-05T00:00:00Z', deleted_at: '2026-03-02T00:00:00Z' }),
  ],
}

const ids = (group) => group.sessions.map((s) => s.run.id)

test('groups by project, newest deletion first', () => {
  const groups = groupTrash(LISTING, 'deleted')
  assert.deepEqual(groups.map((g) => g.project.id), ['gone', 'live'])
  const [gone, live] = groups
  assert.equal(gone.projectTrashed, true)
  assert.equal(live.projectTrashed, false)
  // g1 has no stamp of its own: it takes the project's (03-05), after g2's 03-02.
  assert.deepEqual(ids(gone), ['g1', 'g2'])
  assert.equal(gone.sessions[0].deletedAt, '2026-03-05T00:00:00Z')
  assert.deepEqual(ids(live), ['c', 'a'])
})

test('sorts by creation time when asked', () => {
  const groups = groupTrash(LISTING, 'created')
  // live's newest session (a, 02-10) is older than gone's (g1, 02-20).
  assert.deepEqual(groups.map((g) => g.project.id), ['gone', 'live'])
  assert.deepEqual(ids(groups[0]), ['g1', 'g2'])
  assert.deepEqual(ids(groups[1]), ['a', 'c'])
})

test('a worker whose conductor is in the Trash is listed under it', () => {
  const live = groupTrash(LISTING, 'deleted').find((g) => g.project.id === 'live')
  const conductor = live.sessions.find((s) => s.run.id === 'c')
  assert.deepEqual(conductor.workers.map((w) => w.id), ['w'])
  assert.ok(!ids(live).includes('w'))
  // A worker trashed on its own (conductor live) is a row of its own.
  const alone = groupTrash({
    projects: [project('p')],
    runs: [run('w', 'p', { deleted_at: '2026-03-01T00:00:00Z', conductor_run_id: 'live-conductor' })],
  }, 'deleted')
  assert.deepEqual(ids(alone[0]), ['w'])
})

test('purge counts: a project takes every run, a row takes its workers', () => {
  const groups = groupTrash(LISTING, 'deleted')
  const gone = groups.find((g) => g.project.id === 'gone')
  const live = groups.find((g) => g.project.id === 'live')
  assert.equal(gone.purgeCount, 2)
  // c (1 + 2 workers, one of them maybe live again) + a.
  assert.equal(live.purgeCount, 4)
  assert.equal(sessionsLost(LISTING.runs[1]), 3)
  assert.deepEqual(trashTotals(groups), { projects: 1, sessions: 6 })
})

test('a trashed project without sessions is still a group', () => {
  const groups = groupTrash({
    projects: [project('empty', { deleted_at: '2026-03-09T00:00:00Z' }), project('live')],
    runs: [run('a', 'live', { deleted_at: '2026-03-01T00:00:00Z' })],
  }, 'deleted')
  assert.deepEqual(groups.map((g) => g.project.id), ['empty', 'live'])
  assert.deepEqual(groups[0].sessions, [])
  assert.equal(groups[0].purgeCount, 0)
  assert.deepEqual(trashTotals(groups), { projects: 1, sessions: 1 })
})

test('an empty Trash has no groups', () => {
  assert.deepEqual(groupTrash({ projects: [], runs: [] }, 'deleted'), [])
  assert.deepEqual(trashTotals([]), { projects: 0, sessions: 0 })
})

test('the stored sort falls back to the deletion time', () => {
  assert.equal(parseTrashSort('created'), 'created')
  assert.equal(parseTrashSort('deleted'), 'deleted')
  assert.equal(parseTrashSort('bogus'), 'deleted')
  assert.equal(parseTrashSort(null), 'deleted')
})
