// Unit tests for lib/skillEval.ts (Skill eval dialog of the skill detail).
// Run: node --test src/lib/__tests__/skillEval.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  DEFAULT_EVAL_BUDGET,
  MAX_CASE_PROMPT_CHARS,
  MAX_EVAL_CASES,
  caseProblem,
  evalArgs,
  evalSummary,
  newCaseId,
} from '../skillEval.ts'

const c = (id, prompt = 'fill this PDF form', expect = 'activate') => ({ id, prompt, expect })

test('new case ids take the first free number', () => {
  assert.equal(newCaseId([]), 'c1')
  assert.equal(newCaseId([c('c1'), c('c3')]), 'c2')
  assert.equal(newCaseId([c('c2'), c('c1')]), 'c3')
})

test('cases the backend would refuse are reported, 1-based', () => {
  assert.deepEqual(caseProblem([]), { key: 'noCases' })
  assert.equal(caseProblem(Array.from({ length: MAX_EVAL_CASES }, (_, i) => c(`c${i}`))), null)
  assert.deepEqual(caseProblem(Array.from({ length: MAX_EVAL_CASES + 1 }, (_, i) => c(`c${i}`))), {
    key: 'tooMany',
    max: MAX_EVAL_CASES,
  })
  assert.deepEqual(caseProblem([c('c1'), c('c2', '  \n ')]), { key: 'emptyPrompt', index: 2 })
  // Counted in characters like the backend: an emoji is one, not two UTF-16 units.
  assert.equal(caseProblem([c('c1', '😀'.repeat(MAX_CASE_PROMPT_CHARS))]), null)
  assert.deepEqual(caseProblem([c('c1', 'x'.repeat(MAX_CASE_PROMPT_CHARS + 1))]), {
    key: 'promptTooLong',
    index: 1,
    max: MAX_CASE_PROMPT_CHARS,
  })
})

test('the arguments trim prompts and turn minutes into ms', () => {
  const args = evalArgs('id1', [c('c1', '  hi  ', 'skip')], { maxCalls: 2.7, maxCostUsd: 0.25, maxMinutes: 1.5 })
  assert.deepEqual(args, {
    skillId: 'id1',
    cases: [{ id: 'c1', prompt: 'hi', expect: 'skip' }],
    budget: { max_calls: 2, max_cost_usd: 0.25, max_wallclock_ms: 90_000 },
  })
})

test('a blank or non-positive cap is left to the backend default', () => {
  const { budget } = evalArgs('id1', [c('c1')], { maxCalls: NaN, maxCostUsd: 0, maxMinutes: -1 })
  assert.deepEqual(budget, { max_calls: null, max_cost_usd: null, max_wallclock_ms: null })
})

test('the default form matches the backend defaults', () => {
  const rs = readFileSync(new URL('../../../src-tauri/src/commands/skill_eval.rs', import.meta.url), 'utf8')
  const m = /DEFAULT_BUDGET: Budget = Budget \{ max_calls: (\d+), max_cost_usd: ([\d.]+), max_wallclock_ms: (\d+) \* 60_000 \}/.exec(rs)
  assert.ok(m, 'DEFAULT_BUDGET in skill_eval.rs')
  assert.deepEqual(
    [Number(m[1]), Number(m[2]), Number(m[3])],
    [DEFAULT_EVAL_BUDGET.maxCalls, DEFAULT_EVAL_BUDGET.maxCostUsd, DEFAULT_EVAL_BUDGET.maxMinutes],
  )
  assert.match(rs, new RegExp(`MAX_CASES_PER_RUN: usize = ${MAX_EVAL_CASES};`))
  assert.match(rs, /MAX_CASE_PROMPT_CHARS: usize = 1_000;/)
  assert.equal(MAX_CASE_PROMPT_CHARS, 1000)
})

test('the summary counts each status', () => {
  const r = (status, pass = false) => ({ case_id: 'x', prompt: 'p', expect: 'activate', chosen: null, pass, reason: '', status })
  const run = { passed: 1, results: [r('scored', true), r('scored'), r('skipped'), r('skipped'), r('error')] }
  assert.deepEqual(evalSummary(run), { passed: 1, scored: 2, skipped: 2, errors: 1 })
})
