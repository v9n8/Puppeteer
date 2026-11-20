// RT-11 (#13): a failed `result` with an empty `result` shows the reasons the
// CLI put in `errors` instead of an empty "Failed" box.
// Run: node --test src/lib/__tests__/streamEventsResultErrors.test.mjs
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
const { parseStreamLogWindow } = await import('../streamEvents.ts')

const lastResult = (lines) => parseStreamLogWindow(lines.map((l) => JSON.stringify(l))).entries.filter((e) => e.kind === 'result').at(-1)

test('an empty failed result takes its text from `errors` (25659a17, turn 7)', () => {
  const r = lastResult([{
    type: 'result', subtype: 'error_during_execution', is_error: true, result: '',
    errors: ['prompt_check: the turn was stopped before its prompt was checked'],
  }])
  assert.equal(r.is_error, true)
  assert.equal(r.text, 'prompt_check: the turn was stopped before its prompt was checked')
})

test('several reasons go one per line; an error subtype without is_error counts too', () => {
  const r = lastResult([{ type: 'result', subtype: 'error_during_execution', errors: ['first', ' ', 'second'] }])
  assert.equal(r.text, 'first\nsecond')
})

test('a failed result that has its own text keeps it', () => {
  const r = lastResult([{ type: 'result', subtype: 'success', is_error: true, result: 'API Error: 529', errors: ['other'] }])
  assert.equal(r.text, 'API Error: 529')
})

test('a successful result never shows `errors`', () => {
  const r = lastResult([{ type: 'result', subtype: 'success', is_error: false, result: '', errors: ['noise'] }])
  assert.equal(r.text, '')
})
