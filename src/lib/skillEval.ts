// Skill eval (`run_skill_eval`, gated by `library.skill_eval`): sample
// requests are sent to the translate engine's model, which says which skill it
// would load; each case passes when that matches what the case expects. The
// backend stops at the first spend cap reached (calls, USD, time) and returns
// the cases it did not run as `skipped`. Nothing is stored.

export const CMD_RUN_SKILL_EVAL = 'run_skill_eval'
export const SKILL_EVAL_FEATURE = 'library.skill_eval'

/** Edge limits of `skill_eval.rs` (`MAX_CASES_PER_RUN`, `MAX_CASE_PROMPT_CHARS`). */
export const MAX_EVAL_CASES = 20
export const MAX_CASE_PROMPT_CHARS = 1000

/** `DEFAULT_BUDGET` of `skill_eval.rs`, in the form's units. */
export const DEFAULT_EVAL_BUDGET: EvalBudgetForm = { maxCalls: 20, maxCostUsd: 0.5, maxMinutes: 5 }

export type EvalExpect = 'activate' | 'skip'
export type EvalCaseStatus = 'scored' | 'skipped' | 'error'
/** Which cap ended the run early (`error`: two failed calls in a row). */
export type EvalStop = 'calls' | 'cost' | 'wallclock' | 'error'

export interface EvalCase {
  id: string
  prompt: string
  expect: EvalExpect
}

export interface EvalBudgetForm {
  maxCalls: number
  maxCostUsd: number
  maxMinutes: number
}

export interface EvalCaseResult {
  case_id: string
  prompt: string
  expect: EvalExpect
  /** The skill the model picked; null = none. */
  chosen: string | null
  pass: boolean
  reason: string
  status: EvalCaseStatus
  error?: string
}

export interface SkillEvalRun {
  skill_id: string
  skill_name: string
  engine: string
  model: string
  started_at: number
  finished_at: number
  results: EvalCaseResult[]
  passed: number
  /** Scored cases. */
  total: number
  calls: number
  cost_usd: number
  cost_estimated: boolean
  budget: { max_calls: number; max_cost_usd: number; max_wallclock_ms: number }
  stopped_by?: EvalStop
}

/** First free `c<n>` id. */
export function newCaseId(cases: readonly EvalCase[]): string {
  const taken = new Set(cases.map((c) => c.id))
  let n = 1
  while (taken.has(`c${n}`)) n++
  return `c${n}`
}

export type CaseProblem =
  | { key: 'noCases' }
  | { key: 'tooMany'; max: number }
  | { key: 'emptyPrompt'; index: number }
  | { key: 'promptTooLong'; index: number; max: number }

/** Why the cases can't be sent (the backend would refuse them), or null. `index` is 1-based. */
export function caseProblem(cases: readonly EvalCase[]): CaseProblem | null {
  if (cases.length === 0) return { key: 'noCases' }
  if (cases.length > MAX_EVAL_CASES) return { key: 'tooMany', max: MAX_EVAL_CASES }
  for (const [i, c] of cases.entries()) {
    const prompt = c.prompt.trim()
    if (!prompt) return { key: 'emptyPrompt', index: i + 1 }
    // Characters, as the backend counts them (not UTF-16 units).
    if ([...prompt].length > MAX_CASE_PROMPT_CHARS) return { key: 'promptTooLong', index: i + 1, max: MAX_CASE_PROMPT_CHARS }
  }
  return null
}

/** Arguments of `run_skill_eval`. A cap left blank (NaN / not positive) takes the backend default. */
export function evalArgs(skillId: string, cases: readonly EvalCase[], budget: EvalBudgetForm) {
  const positive = (n: number) => (Number.isFinite(n) && n > 0 ? n : null)
  const minutes = positive(budget.maxMinutes)
  return {
    skillId,
    cases: cases.map((c) => ({ id: c.id, prompt: c.prompt.trim(), expect: c.expect })),
    budget: {
      max_calls: positive(Math.floor(budget.maxCalls)),
      max_cost_usd: positive(budget.maxCostUsd),
      max_wallclock_ms: minutes === null ? null : Math.round(minutes * 60_000),
    },
  }
}

export interface EvalSummary {
  passed: number
  scored: number
  skipped: number
  errors: number
}

export function evalSummary(run: SkillEvalRun): EvalSummary {
  const count = (s: EvalCaseStatus) => run.results.filter((r) => r.status === s).length
  return { passed: run.passed, scored: count('scored'), skipped: count('skipped'), errors: count('error') }
}
