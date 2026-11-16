// Unit tests for lib/trash.ts (Trash screen grouping + sorting).
// Run: node --test src/lib/__tests__/
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  contentFilter,
  filterTrash,
  groupActionTargets,
  groupTrash,
  highlightParts,
  initialOpenGroups,
  parseTrashSearchMode,
  parseTrashSort,
  sessionsLost,
  trashTotals,
} from '../trash.ts'

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
  const groups = groupTrash(LISTING, 'deleted-desc')
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
  const groups = groupTrash(LISTING, 'created-desc')
  // live's newest session (a, 02-10) is older than gone's (g1, 02-20).
  assert.deepEqual(groups.map((g) => g.project.id), ['gone', 'live'])
  assert.deepEqual(ids(groups[0]), ['g1', 'g2'])
  assert.deepEqual(ids(groups[1]), ['a', 'c'])
})

test('a worker whose conductor is in the Trash is listed under it', () => {
  const live = groupTrash(LISTING, 'deleted-desc').find((g) => g.project.id === 'live')
  const conductor = live.sessions.find((s) => s.run.id === 'c')
  assert.deepEqual(conductor.workers.map((w) => w.id), ['w'])
  assert.ok(!ids(live).includes('w'))
  // A worker trashed on its own (conductor live) is a row of its own.
  const alone = groupTrash({
    projects: [project('p')],
    runs: [run('w', 'p', { deleted_at: '2026-03-01T00:00:00Z', conductor_run_id: 'live-conductor' })],
  }, 'deleted-desc')
  assert.deepEqual(ids(alone[0]), ['w'])
})

test('purge counts: a project takes every run, a row takes its workers', () => {
  const groups = groupTrash(LISTING, 'deleted-desc')
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
  }, 'deleted-desc')
  assert.deepEqual(groups.map((g) => g.project.id), ['empty', 'live'])
  assert.deepEqual(groups[0].sessions, [])
  assert.equal(groups[0].purgeCount, 0)
  assert.deepEqual(trashTotals(groups), { projects: 1, sessions: 1 })
})

test('an empty Trash has no groups', () => {
  assert.deepEqual(groupTrash({ projects: [], runs: [] }, 'deleted-desc'), [])
  assert.deepEqual(trashTotals([]), { projects: 0, sessions: 0 })
})

test('sorts oldest first when asked', () => {
  const deleted = groupTrash(LISTING, 'deleted-asc')
  assert.deepEqual(deleted.map((g) => g.project.id), ['live', 'gone'])
  assert.deepEqual(ids(deleted[0]), ['a', 'c'])
  assert.deepEqual(ids(deleted[1]), ['g2', 'g1'])
  const created = groupTrash(LISTING, 'created-asc')
  assert.deepEqual(created.map((g) => g.project.id), ['live', 'gone'])
  assert.deepEqual(ids(created[0]), ['c', 'a'])
  assert.deepEqual(ids(created[1]), ['g2', 'g1'])
})

test('the stored sort reads the old values and falls back to newest deleted', () => {
  assert.equal(parseTrashSort('created'), 'created-desc')
  assert.equal(parseTrashSort('deleted'), 'deleted-desc')
  assert.equal(parseTrashSort('created-asc'), 'created-asc')
  assert.equal(parseTrashSort('deleted-asc'), 'deleted-asc')
  assert.equal(parseTrashSort('bogus'), 'deleted-desc')
  assert.equal(parseTrashSort(null), 'deleted-desc')
})

// Search: names of the runs, as the screen would show them.
const NAMED = {
  projects: [
    project('gone', { name: 'Puppeteer', deleted_at: '2026-03-05T00:00:00Z' }),
    project('live', { name: 'Billing' }),
  ],
  runs: [
    run('a', 'live', { title: 'Hóa đơn tháng 5', deleted_at: '2026-03-01T00:00:00Z' }),
    run('c', 'live', { title: 'Conductor', deleted_at: '2026-03-03T00:00:00Z', conductor_run_id: 'c', worker_count: 1 }),
    run('w', 'live', { title: 'Fix README', deleted_at: '2026-03-03T00:00:00Z', conductor_run_id: 'c' }),
    run('g1', 'gone', { title: 'Plan' }),
    run('g2', 'gone', { title: 'README tidy', deleted_at: '2026-03-02T00:00:00Z' }),
  ],
}
const label = (r) => r.title ?? r.id
const NAMED_GROUPS = groupTrash(NAMED, 'deleted-desc')

test('no query keeps every block untouched', () => {
  const found = filterTrash(NAMED_GROUPS, '  ', label)
  assert.equal(found.length, 2)
  assert.ok(found.every((e) => e.matched === null && e.group === e.full))
})

test('a project name keeps the whole block, accents and case ignored', () => {
  const found = filterTrash(NAMED_GROUPS, 'BILL', label)
  assert.equal(found.length, 1)
  assert.equal(found[0].matched, null)
  assert.deepEqual(ids(found[0].group), ['c', 'a'])
})

test('a session name keeps only that row; folded so "hoa don" finds "Hóa đơn"', () => {
  const found = filterTrash(NAMED_GROUPS, 'hoa don', label)
  assert.equal(found.length, 1)
  assert.deepEqual(ids(found[0].group), ['a'])
  assert.deepEqual([...found[0].matched], ['a'])
})

test('a worker name matches the row of its conductor', () => {
  const found = filterTrash(NAMED_GROUPS, 'readme', label)
  const live = found.find((e) => e.full.project.id === 'live')
  assert.deepEqual(ids(live.group), ['c'])
  assert.equal(live.group.sessions[0].workers.length, 1)
})

test('no match gives no blocks', () => {
  const found = filterTrash(NAMED_GROUPS, 'zzz', label)
  assert.deepEqual(found, [])
})

test('whole-block targets without a search', () => {
  const [gone, live] = NAMED_GROUPS
  const all = groupActionTargets(gone, null)
  assert.deepEqual([all.partial, all.wholeProject, all.purgeCount, all.remaining], [false, true, 2, 0])
  const alive = groupActionTargets(live, null)
  assert.deepEqual([alive.partial, alive.wholeProject, alive.sessions.length], [false, false, 2])
})

test('a search narrows the targets of a live project and counts what stays', () => {
  const live = NAMED_GROUPS.find((g) => g.project.id === 'live')
  const t = groupActionTargets(live, new Set(['a']))
  assert.deepEqual(t.sessions.map((s) => s.run.id), ['a'])
  assert.deepEqual([t.partial, t.wholeProject, t.purgeCount, t.remaining, t.restoredWithProject], [true, false, 1, 1, 0])
  // The conductor row takes its workers in the purge count.
  assert.equal(groupActionTargets(live, new Set(['c'])).purgeCount, 2)
})

test('a part of a trashed project: purge leaves it, restore brings the project and its project-only rows', () => {
  const gone = NAMED_GROUPS.find((g) => g.project.id === 'gone')
  const t = groupActionTargets(gone, new Set(['g2']))
  assert.deepEqual(t.sessions.map((s) => s.run.id), ['g2'])
  assert.equal(t.wholeProject, false)
  assert.equal(t.remaining, 1)
  // g1 has no stamp of its own: it comes back with the project.
  assert.equal(t.restoredWithProject, 1)
  assert.equal(groupActionTargets(gone, new Set(['g1'])).restoredWithProject, 0)
})

test('the first block starts open; stored choices win', () => {
  assert.deepEqual([...initialOpenGroups(NAMED_GROUPS, null)], ['gone'])
  assert.deepEqual([...initialOpenGroups(NAMED_GROUPS, { gone: false, live: true })], ['live'])
  assert.deepEqual([...initialOpenGroups(NAMED_GROUPS, { live: true })].sort(), ['gone', 'live'])
  assert.deepEqual([...initialOpenGroups([], null)], [])
})

test('highlightParts flags the first match and keeps the original text', () => {
  assert.deepEqual(highlightParts('Fix README now', 'readme'), [
    { text: 'Fix ', hit: false },
    { text: 'README', hit: true },
    { text: ' now', hit: false },
  ])
  assert.deepEqual(highlightParts('Hóa đơn', 'hoa'), [
    { text: 'Hóa', hit: true },
    { text: ' đơn', hit: false },
  ])
  assert.deepEqual(highlightParts('Plan', 'zzz'), [{ text: 'Plan', hit: false }])
  assert.deepEqual(highlightParts('Plan', ''), [{ text: 'Plan', hit: false }])
})

test('content search keeps only sessions with hits, a worker counts for its conductor', () => {
  const groups = groupTrash(LISTING, 'deleted-desc')
  const { entries, hits } = contentFilter(groups, [
    { runId: 'w', hits: [{ snippet: 'from worker' }] },
    { runId: 'c', hits: [{ snippet: 'one' }, { snippet: 'two' }] },
    { runId: 'g2', hits: [{ snippet: 'in gone' }] },
    { runId: 'not-listed', hits: [{ snippet: 'live run' }] },
  ])
  assert.deepEqual(entries.map((e) => e.full.project.id), ['gone', 'live'])
  const [gone, live] = entries
  // A session of a trashed project: its row only, so the block's buttons are partial.
  assert.deepEqual(ids(gone.group), ['g2'])
  assert.equal(groupActionTargets(gone.full, gone.matched).partial, true)
  assert.equal(groupActionTargets(gone.full, gone.matched).wholeProject, false)
  assert.deepEqual(ids(live.group), ['c'])
  // Three messages in all, only the first two shown (the conductor's own first).
  assert.deepEqual(hits.get('c'), { snippets: ['one', 'two'], count: 3 })
  assert.deepEqual(hits.get('g2'), { snippets: ['in gone'], count: 1 })
  assert.equal(hits.has('a'), false)
})

test('content search with no hit lists nothing', () => {
  const groups = groupTrash(LISTING, 'deleted-desc')
  assert.deepEqual(contentFilter(groups, []), { entries: [], hits: new Map() })
})

test('the stored search mode falls back to name', () => {
  assert.equal(parseTrashSearchMode('content'), 'content')
  assert.equal(parseTrashSearchMode('name'), 'name')
  assert.equal(parseTrashSearchMode('x'), 'name')
  assert.equal(parseTrashSearchMode(null), 'name')
})
