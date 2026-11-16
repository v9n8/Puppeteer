// Unit tests for lib/turnStatus.ts (the turn status bar's state and step list) and
// lib/stepMarks.ts (the log's step timestamps).
// Run: node --test src/lib/__tests__/turnStatus.test.mjs
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
const { lastTurn, stepKind, toolDetail, todoProgress, turnSteps, turnStatus, turnTodos } = await import('../turnStatus.ts')
const { applyStreamEvent, createStreamState, parseStreamLog } = await import('../streamEvents.ts')
const { applyStepMark } = await import('../stepMarks.ts')

const user = (text = 'go', extra = {}) => ({ kind: 'user', text, ...extra })
const tool = (name, input = {}, extra = {}) => ({ kind: 'tool', id: `${name}-${Math.random()}`, name, input, ...extra })
const ok = { content: 'ok', is_error: false }
const fail = { content: 'boom', is_error: true }
const result = (extra = {}) => ({ kind: 'result', text: '', is_error: false, ...extra })
const status = (o) => turnStatus({ entries: [], running: false, ...o })

test('none: no prompt yet, and loading wins over everything', () => {
  assert.deepEqual(status({}), { kind: 'none' })
  assert.deepEqual(status({ entries: [{ kind: 'system' }] }), { kind: 'none' })
  assert.deepEqual(status({ entries: [user()], loading: true }), { kind: 'loading' })
})

test('done: turn number, wall time and tokens of the turn', () => {
  const entries = [user(), result(), user(), tool('Read', {}, { result: ok }), result({ duration_ms: 6100, total_tokens: 3240 })]
  assert.deepEqual(status({ entries, runStatus: 'done' }), { kind: 'done', turn: 2, durationMs: 6100, tokens: 3240 })
  assert.deepEqual(status({ entries: [user(), result()], runStatus: 'done' }), { kind: 'done', turn: 1, durationMs: null, tokens: null })
})

test('the prompt\'s own turn number wins when the log has it', () => {
  assert.equal(lastTurn([user('a', { turn: 7 })]).turn, 7)
  assert.equal(lastTurn([user('a'), user('b')]).turn, 2)
  assert.equal(lastTurn([{ kind: 'text', text: 'x' }]), null)
})

test('tool: the running call, its input and start time', () => {
  const entries = [user(), tool('Bash', { command: 'pnpm   test auth' }, { startedAt: 1000, elapsedSecs: 42 })]
  assert.deepEqual(status({ entries, running: true }), {
    kind: 'tool', turn: 1, tool: 'Bash', detail: 'pnpm test auth', startedAt: 1000, elapsedSecs: 42,
  })
  // A finished call, then more thinking: not "in" the tool any more.
  const after = [user(), tool('Bash', { command: 'ls' }, { result: ok }), { kind: 'thinking', text: '…' }]
  assert.equal(status({ entries: after, running: true }).kind, 'thinking')
})

test('thinking: with and without an API retry', () => {
  assert.deepEqual(status({ entries: [user()], running: true }), { kind: 'thinking', turn: 1, retry: null })
  const retry = { kind: 'retry', attempt: 2, maxRetries: 5, delayMs: 8000, errorStatus: 529 }
  assert.deepEqual(status({ entries: [user()], running: true, liveStatus: retry }), {
    kind: 'thinking', turn: 1, retry: { attempt: 2, maxRetries: 5, delaySecs: 8 },
  })
  assert.equal(status({ entries: [user()], running: true, liveStatus: { kind: 'compacting' } }).retry, null)
})

test('permission: the pending request beats a running tool', () => {
  const entries = [user(), tool('Bash', { command: 'git push origin main' })]
  const pending = { toolName: 'Bash', toolInput: { command: 'git push origin main' } }
  assert.deepEqual(status({ entries, running: true, pending }), { kind: 'permission', turn: 1, detail: 'git push origin main' })
  assert.equal(status({ entries, running: true, pending: { toolName: 'mcp__x', toolInput: {} } }).detail, 'mcp__x')
})

test('question: the agent asks the user', () => {
  const pending = { toolName: 'AskUserQuestion', toolInput: { questions: [{ question: 'Which   database?' }] } }
  assert.deepEqual(status({ entries: [user()], running: true, pending }), { kind: 'question', turn: 1, text: 'Which database?' })
})

test('error: an error result, a stray error entry, or a failed run', () => {
  assert.deepEqual(status({ entries: [user(), result({ is_error: true, text: 'API 529 Overloaded' })], runStatus: 'failed' }), {
    kind: 'error', turn: 1, text: 'API 529 Overloaded',
  })
  assert.deepEqual(status({ entries: [user(), { kind: 'error', text: 'spawn failed' }], runStatus: 'failed' }), {
    kind: 'error', turn: 1, text: 'spawn failed',
  })
  assert.deepEqual(status({ entries: [user()], runStatus: 'failed' }), { kind: 'error', turn: 1, text: '' })
})

test('cancelled: the user stopped it, or the app closed / quota stopped it', () => {
  assert.deepEqual(status({ entries: [user(), user()], runStatus: 'cancelled' }), { kind: 'cancelled', turn: 2 })
  assert.equal(status({ entries: [user()], runStatus: 'done', endReason: 'app_closed' }).kind, 'cancelled')
  assert.equal(status({ entries: [user()], runStatus: 'done', endReason: 'quota' }).kind, 'cancelled')
})

test('turnSteps: only the last turn, with state and duration', () => {
  const entries = [
    user(),
    tool('Read', { file_path: 'old.ts' }, { result: ok }),
    user(),
    tool('Read', { file_path: 'a.ts' }, { result: ok, startedAt: 1000, endedAt: 1450 }),
    tool('Edit', { file_path: 'a.ts' }, { result: fail, elapsedSecs: 3 }),
    tool('Bash', { command: 'pnpm test' }, { startedAt: 5000 }),
    tool('mcp__x__y', {}, { result: ok }),
  ]
  const steps = turnSteps(entries)
  assert.deepEqual(steps.map((s) => [s.kind, s.detail, s.state, s.durationMs, s.startedAt]), [
    ['read', 'a.ts', 'done', 450, null],
    ['edit', 'a.ts', 'error', 3000, null],
    ['bash', 'pnpm test', 'running', null, 5000],
    ['other', '', 'done', null, null],
  ])
  assert.deepEqual(turnSteps([]), [])
})

test('turnTodos: the last TodoWrite of the last turn, none when it wrote none', () => {
  const write = (todos) => tool('TodoWrite', { todos })
  const entries = [
    user(),
    write([{ content: 'old', status: 'completed' }]),
    user(),
    write([{ content: 'a', status: 'completed' }, { content: 'b', status: 'pending' }]),
    write([{ content: 'a', status: 'completed' }, { content: 'b', status: 'in_progress' }, { content: ' ', status: 'pending' }]),
  ]
  const todos = turnTodos(entries)
  assert.deepEqual(todos.map((t) => t.status), ['completed', 'in_progress'])
  assert.deepEqual(todoProgress(todos), { done: 1, total: 2 })
  assert.deepEqual(turnTodos([user(), tool('Read')]), [])
})

test('stepKind and toolDetail', () => {
  assert.equal(stepKind('MultiEdit'), 'edit')
  assert.equal(stepKind('Glob'), 'search')
  assert.equal(stepKind('mcp__a__b'), 'other')
  assert.equal(toolDetail({ file_path: '/x/y.ts', command: 'c' }), 'c')
  assert.equal(toolDetail({ pattern: 'a'.repeat(300) }, 20).length, 20)
  assert.equal(toolDetail(null), '')
})

test('step marks stamp the tool entries; unknown ids and bad marks do nothing', () => {
  const state = createStreamState()
  applyStreamEvent(state, { type: 'assistant', message: { content: [{ type: 'tool_use', id: 't1', name: 'Bash', input: {} }] } })
  applyStreamEvent(state, { type: '_puppeteer_step_at', phase: 'start', ids: ['t1', 'nope'], at: 1000 })
  applyStreamEvent(state, { type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 't1', content: 'ok' }] } })
  applyStreamEvent(state, { type: '_puppeteer_step_at', phase: 'end', ids: ['t1'], at: 2500 })
  applyStreamEvent(state, { type: '_puppeteer_step_at', phase: 'sideways', ids: ['t1'], at: 9 })
  applyStreamEvent(state, { type: '_puppeteer_step_at', phase: 'end', ids: 'x', at: 9 })
  assert.equal(state.entries.length, 1)
  const [e] = state.entries
  assert.equal(e.startedAt, 1000)
  assert.equal(e.endedAt, 2500)
  assert.equal(e.result.content, 'ok')
  assert.equal(turnSteps([user(), e])[0].durationMs, 1500)
  applyStepMark(state, { phase: 'end', ids: ['t1'], at: 2500 })
  assert.equal(state.entries[0], e)
})

test('a saved log with step marks parses to the same entries as one without', () => {
  const lines = [
    { type: 'user', message: { role: 'user', content: 'run it' } },
    { type: 'assistant', message: { content: [{ type: 'tool_use', id: 't1', name: 'Bash', input: { command: 'ls' } }] } },
    { type: '_puppeteer_step_at', phase: 'start', ids: ['t1'], at: 10 },
    { type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 't1', content: 'a' }] } },
    { type: '_puppeteer_step_at', phase: 'end', ids: ['t1'], at: 90 },
  ]
  const withMarks = parseStreamLog(lines.map((l) => JSON.stringify(l)).join('\n')).entries
  const without = parseStreamLog(lines.filter((l) => l.type !== '_puppeteer_step_at').map((l) => JSON.stringify(l)).join('\n')).entries
  assert.equal(withMarks.length, without.length)
  assert.equal(withMarks[1].endedAt - withMarks[1].startedAt, 80)
  assert.equal(without[1].startedAt, undefined)
  assert.equal(turnSteps(without)[0].durationMs, null)
})
