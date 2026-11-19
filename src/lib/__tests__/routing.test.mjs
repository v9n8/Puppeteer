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
