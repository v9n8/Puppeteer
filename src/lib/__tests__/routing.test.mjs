// Unit tests for lib/routing.ts (Settings › Routing, QO-06).
// Run: node --test src/lib/__tests__/routing.test.mjs
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
const { adoptionRates, percent, routingReportCells, switchOn } = await import('../routing.ts')

const group = (over = {}) => ({
  task_kind: 'test',
  model: 'sonnet',
  effort: 'medium',
  runs: 5,
  counted: 4,
  successes: 2,
  avg_tokens: 1501,
  retried: 1,
  by_agent: 3,
  ...over,
})

test('QO-AC49: a report line shows runs, success, average tokens, retries and the agent share', () => {
  assert.deepEqual(routingReportCells(group()), {
    kind: 'test',
    model: 'sonnet',
    effort: 'medium',
    runs: '5',
    success: '50%',
    avgTokens: '1.5k',
    retried: '20%',
    byAgent: '60%',
  })
})

test('parts that are not known show a dash', () => {
  const cells = routingReportCells(group({ model: '', effort: '', counted: 0, successes: 0, avg_tokens: null }))
  assert.equal(cells.model, '—')
  assert.equal(cells.effort, '—')
  assert.equal(cells.success, '—', 'no run counted: no rate')
  assert.equal(cells.avgTokens, '—')
})

test('percentages round and never divide by zero', () => {
  assert.equal(percent(19, 20), '95%')
  assert.equal(percent(2, 3), '67%')
  assert.equal(percent(0, 4), '0%')
  assert.equal(percent(1, 0), '—')
})

test('QO-NFR13: the three adoption rates', () => {
  assert.deepEqual(adoptionRates({ calls: 4, with_model: 2, with_effort: 3, with_both: 1 }), {
    model: '50%',
    effort: '75%',
    both: '25%',
  })
  assert.deepEqual(adoptionRates({ calls: 0, with_model: 0, with_effort: 0, with_both: 0 }), {
    model: '—',
    effort: '—',
    both: '—',
  })
})

test('a switch is on unless it says false', () => {
  assert.equal(switchOn(undefined), true)
  assert.equal(switchOn(''), true)
  assert.equal(switchOn('true'), true)
  assert.equal(switchOn('false'), false)
  assert.equal(switchOn(' false '), false)
})

const { changeText, isNewNotice, noticeSummary, sourceLabelKey, tierText } = await import('../routing.ts')

const change = (kind, from, to, basis = null) => ({ kind, from, to, basis })
const tier = (model, effort) => ({ model, effort })

test('QO-AC63: the toast sums up every move of the new version', () => {
  const changes = [
    change('test', tier('sonnet', 'medium'), tier('sonnet', 'low'), { samples: 20, successes: 19, failures: 1 }),
    change('review', tier('sonnet', 'medium'), tier('sonnet', 'high')),
  ]
  assert.equal(noticeSummary(changes), 'test: sonnet/medium → sonnet/low; review: sonnet/medium → sonnet/high')
  assert.equal(noticeSummary([]), '')
})

test('a kind with no tier reads as a dash', () => {
  assert.equal(tierText(null), '—')
  assert.equal(changeText(change('other', null, tier('haiku', 'low'))), 'other: — → haiku/low')
})

test('a notice is shown once, even when the start-up read and the event race', () => {
  const notice = { version: 3, changes: [] }
  assert.equal(isNewNotice(notice, 0), true)
  assert.equal(isNewNotice(notice, 3), false, 'already shown')
  assert.equal(isNewNotice(null, 0), false, 'take_routing_notice gave nothing: it was taken before')
  assert.equal(isNewNotice(undefined, 0), false)
})

test('version sources have labels; an unknown one is shown as stored', () => {
  assert.equal(sourceLabelKey('default'), 'routing.versions.sources.default')
  assert.equal(sourceLabelKey('auto'), 'routing.versions.sources.auto')
  assert.equal(sourceLabelKey('revert'), 'routing.versions.sources.revert')
  assert.equal(sourceLabelKey('manual'), null)
})
