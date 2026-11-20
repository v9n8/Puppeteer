// RT-09 (#14): the context meter. From the repro in
// .puppeteer/scratch/report-tasks-2026-11-20/contextMeter.repro.test.mjs, with
// the first case's expectation turned around: a subagent's usage no longer
// overwrites the main window's figure.
// Run: node --test src/lib/__tests__/streamEventsContext.test.mjs
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
const { extractContextTokens, parseStreamLogWindow, applyContextUsage } = await import('../streamEvents.ts')
const { meterReading } = await import('../contextLimits.ts')

const main = (n) => JSON.stringify({ type: 'assistant', parent_tool_use_id: null, message: { role: 'assistant', content: [], usage: { input_tokens: 10, cache_read_input_tokens: n, output_tokens: 5 } } })
const sub = (n) => JSON.stringify({ type: 'assistant', parent_tool_use_id: 'toolu_task', message: { role: 'assistant', content: [], usage: { input_tokens: 10, cache_read_input_tokens: n, output_tokens: 5 } } })
const toolResult = JSON.stringify({ type: 'user', parent_tool_use_id: null, message: { role: 'user', content: [{ type: 'tool_result', tool_use_id: 'x', content: 'ok' }] } })

test('a subagent assistant usage does not overwrite the main window figure', () => {
  const s = parseStreamLogWindow([main(150000), sub(3000)])
  assert.equal(s.contextTokens, 150015)
  assert.equal(extractContextTokens(JSON.parse(sub(3000))), null)
  assert.equal(extractContextTokens(JSON.parse(main(3000))), 3015)
})

test('a tail window with no assistant usage gives 0 (unknown)', () => {
  const s = parseStreamLogWindow([toolResult, toolResult])
  assert.equal(s.contextTokens, 0)
})

const boundary = (post) =>
  JSON.stringify({ type: 'system', subtype: 'compact_boundary', ...(post ? { compact_metadata: { trigger: 'manual', pre_tokens: 150015, post_tokens: post } } : {}) })
const compactResult = JSON.stringify({ type: 'result', is_error: false, num_turns: 0, usage: { input_tokens: 0, cache_read_input_tokens: 0, output_tokens: 0 } })

test('after a compact boundary the meter keeps the post-compact estimate, marked stale', () => {
  const s = parseStreamLogWindow([main(150000), boundary(17190), compactResult])
  assert.equal(s.contextTokens, 17190)
  assert.equal(s.contextStale, true)
})

test('a boundary without post_tokens keeps the figure before it, marked stale', () => {
  const s = parseStreamLogWindow([main(150000), boundary(null)])
  assert.equal(s.contextTokens, 150015)
  assert.equal(s.contextStale, true)
})

test('the next reply measures again and clears the stale mark', () => {
  const s = parseStreamLogWindow([main(150000), boundary(17190), main(20000)])
  assert.equal(s.contextTokens, 20015)
  assert.equal(s.contextStale, false)
})

test('the live store path: applyContextUsage on a live session', () => {
  const m = { contextTokens: 0, contextStale: false, sawAssistantUsage: false }
  applyContextUsage(m, JSON.parse(main(150000)))
  applyContextUsage(m, JSON.parse(boundary(17190)))
  applyContextUsage(m, JSON.parse(compactResult))
  assert.deepEqual(m, { contextTokens: 17190, contextStale: true, sawAssistantUsage: false })
  applyContextUsage(m, JSON.parse(main(18000)))
  assert.deepEqual(m, { contextTokens: 18015, contextStale: false, sawAssistantUsage: true })
})

test('the meter does not take a live 0 (not measured yet) over the known figure', () => {
  const live = (n, stale = false) => ({ contextTokens: n, contextStale: stale })
  const known = (n, stale = false) => ({ tokens: n, stale })
  assert.deepEqual(meterReading(live(0), known(120000)), { tokens: 120000, stale: false })
  assert.deepEqual(meterReading(undefined, known(120000)), { tokens: 120000, stale: false })
  assert.deepEqual(meterReading(live(90000), known(120000)), { tokens: 90000, stale: false })
  assert.deepEqual(meterReading(live(0), known(0)), { tokens: 0, stale: false })
})

test('the meter stays after /compact, marked stale, until a reply measures', () => {
  const live = (n, stale = false) => ({ contextTokens: n, contextStale: stale })
  const known = (n, stale = false) => ({ tokens: n, stale })
  // Live compacted with an estimate: shown, stale.
  assert.deepEqual(meterReading(live(17190, true), known(150015)), { tokens: 17190, stale: true })
  // Live compacted before it had any figure: the known one predates it.
  assert.deepEqual(meterReading(live(0, true), known(150015)), { tokens: 150015, stale: true })
  // History window ending in a compaction.
  assert.deepEqual(meterReading(undefined, known(17190, true)), { tokens: 17190, stale: true })
})
