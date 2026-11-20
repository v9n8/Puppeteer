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
const { extractContextTokens, parseStreamLogWindow } = await import('../streamEvents.ts')
const { meterTokens } = await import('../contextLimits.ts')

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

test('a compact boundary resets to 0 until the next assistant usage', () => {
  const s = parseStreamLogWindow([main(150000), JSON.stringify({ type: 'system', subtype: 'compact_boundary' })])
  assert.equal(s.contextTokens, 0)
})

test('the meter does not take a live 0 (not measured yet) over the known figure', () => {
  assert.equal(meterTokens(0, 120000), 120000)
  assert.equal(meterTokens(undefined, 120000), 120000)
  assert.equal(meterTokens(90000, 120000), 90000)
  assert.equal(meterTokens(0, 0), 0)
})
