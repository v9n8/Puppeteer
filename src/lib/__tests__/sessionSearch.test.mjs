// Unit tests for lib/sessionSearch.ts (search across sessions: grouping, snippet
// highlight, the route query that opens a run at a hit).
// Run: node --test src/lib/__tests__/sessionSearch.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  groupSessionHits,
  hitFromRouteQuery,
  hitRouteQuery,
  snippetParts,
} from '../sessionSearch.ts'

const hit = (run_id, offset, extra = {}) => ({
  run_id,
  project_id: 'p1',
  project_name: 'Alpha',
  title: `Run ${run_id}`,
  engine: 'claude',
  run_type: 'session',
  activity_at: '2026-11-08T10:00:00Z',
  offset,
  snippet: `snippet ${run_id}@${offset}`,
  ...extra,
})

test('groupSessionHits groups per run, keeping run order and log order', () => {
  const groups = groupSessionHits([
    hit('r2', 0, { project_id: 'p2', project_name: 'Beta', engine: 'codex', title: null }),
    hit('r2', 512, { project_id: 'p2', project_name: 'Beta', engine: 'codex', title: null }),
    hit('r1', 40),
    // Not contiguous: still joins the first group of its run.
    hit('r2', 900, { project_id: 'p2', project_name: 'Beta', engine: 'codex', title: null }),
  ])
  assert.deepEqual(
    groups.map((g) => [g.runId, g.hits.map((h) => h.offset)]),
    [
      ['r2', [0, 512, 900]],
      ['r1', [40]],
    ],
  )
  assert.deepEqual(groups[0], {
    runId: 'r2',
    projectId: 'p2',
    projectName: 'Beta',
    title: null,
    engine: 'codex',
    runType: 'session',
    activityAt: '2026-11-08T10:00:00Z',
    hits: [
      { offset: 0, snippet: 'snippet r2@0' },
      { offset: 512, snippet: 'snippet r2@512' },
      { offset: 900, snippet: 'snippet r2@900' },
    ],
  })
})

test('groupSessionHits of no hits is empty', () => {
  assert.deepEqual(groupSessionHits([]), [])
})

test('snippetParts marks every match, ignoring case', () => {
  assert.deepEqual(snippetParts('Run the Migration, then migration again', 'migration'), [
    { text: 'Run the ', match: false },
    { text: 'Migration', match: true },
    { text: ', then ', match: false },
    { text: 'migration', match: true },
    { text: ' again', match: false },
  ])
})

test('snippetParts at the edges, with no match, and with an empty query', () => {
  assert.deepEqual(snippetParts('abc', 'abc'), [{ text: 'abc', match: true }])
  assert.deepEqual(snippetParts('…foo bar', 'bar'), [
    { text: '…foo ', match: false },
    { text: 'bar', match: true },
  ])
  // The index folds case a little differently: the snippet may not contain the query.
  assert.deepEqual(snippetParts('nothing here', 'xyz'), [{ text: 'nothing here', match: false }])
  assert.deepEqual(snippetParts('text', ''), [{ text: 'text', match: false }])
  assert.deepEqual(snippetParts('', 'q'), [])
})

test('a hit survives the route query round trip', () => {
  const q = hitRouteQuery({ offset: 1234, find: 'two words' })
  assert.deepEqual(q, { offset: '1234', find: 'two words' })
  assert.deepEqual(hitFromRouteQuery(q), { offset: 1234, find: 'two words' })
  assert.deepEqual(hitFromRouteQuery(hitRouteQuery({ offset: 0, find: 'x' })), { offset: 0, find: 'x' })
})

test('hitFromRouteQuery rejects anything that is not a hit', () => {
  assert.equal(hitFromRouteQuery({}), null)
  assert.equal(hitFromRouteQuery({ offset: '12' }), null)
  assert.equal(hitFromRouteQuery({ find: 'x' }), null)
  for (const offset of ['', '-1', '1.5', '1e3', ' 1', 'abc', '99999999999999999999']) {
    assert.equal(hitFromRouteQuery({ offset, find: 'x' }), null, offset)
  }
  // vue-router gives repeated keys as arrays.
  assert.equal(hitFromRouteQuery({ offset: ['1', '2'], find: 'x' }), null)
  assert.equal(hitFromRouteQuery({ offset: '1', find: null }), null)
})
