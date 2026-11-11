// The Codex sidecar's translated stream (AC-15, see sidecar-codex/test-items.mjs
// for what it emits) renders like Claude's: text streams then settles, tool
// cards open from item/started and get their result, the plan is a TodoWrite
// checklist, and a reconnect shows as a retry status until output resumes.
// Run: node --test src/lib/__tests__/streamEventsCodex.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { register } from 'node:module'

// streamEvents.ts imports siblings without an extension (Vite style); resolve
// those to `.ts` so Node's type stripping can load the module graph.
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
const { applyStreamEvent, createStreamState, nextLiveStatus, parseStreamLog } = await import('../streamEvents.ts')

const se = (event) => ({ type: 'stream_event', event, parent_tool_use_id: null })
const use = (id, name, input) => ({ type: 'assistant', message: { role: 'assistant', content: [{ type: 'tool_use', id, name, input }] } })
const result = (id, content, isError = false) => ({ type: 'user', message: { role: 'user', content: [{ type: 'tool_result', tool_use_id: id, content, is_error: isError }] } })

const todos = [
  { content: 'Read the code', status: 'completed', activeForm: 'Read the code' },
  { content: 'Write the test', status: 'in_progress', activeForm: 'Write the test' },
]

// In the order the Codex sidecar emits them for one turn.
const codexTurn = [
  { type: 'system', subtype: 'init', session_id: 'fixture-thread', model: 'gpt-5', cwd: '/w', tools: ['Bash', 'Edit'] },
  se({ type: 'content_block_start', index: 0, content_block: { type: 'thinking', thinking: '' } }),
  se({ type: 'content_block_delta', index: 0, delta: { type: 'thinking_delta', thinking: 'Look' } }),
  { type: 'assistant', message: { role: 'assistant', content: [{ type: 'thinking', thinking: 'Look around' }] } },
  use('c1', 'Bash', { command: 'ls', description: 'ls' }),
  result('c1', 'a.txt\n'),
  use('m1', 'mcp__puppeteer__session_spawn', { title: 'worker' }),
  result('m1', [{ type: 'text', text: 'spawned' }]),
  use('k1', 'Task', { subagent_type: 'spawnAgent', description: 'spawnAgent · gpt-5 · t2', prompt: 'Check the tests' }),
  result('k1', 't2: completed — All green'),
  use('w1', 'WebSearch', { query: 'codex app-server' }),
  result('w1', 'Searched the web for "codex app-server"'),
  use('plan-0', 'TodoWrite', { todos }),
  result('plan-0', 'Plan updated.'),
  use('p1', 'ExitPlanMode', { plan: '# Plan' }),
  result('p1', ''),
  { type: 'system', subtype: 'status', status: 'compacting' },
  { type: 'system', subtype: 'compact_boundary', compact_metadata: { trigger: 'auto' } },
  { type: 'system', subtype: 'api_retry', attempt: 1, max_retries: 5, retry_delay_ms: 0, error_status: 502, error: 'responseStreamDisconnected' },
  se({ type: 'content_block_start', index: 1, content_block: { type: 'text', text: '' } }),
  se({ type: 'content_block_delta', index: 1, delta: { type: 'text_delta', text: 'Hel' } }),
  se({ type: 'content_block_delta', index: 1, delta: { type: 'text_delta', text: 'lo' } }),
  { type: 'assistant', message: { id: 'codex-1', role: 'assistant', content: [{ type: 'text', text: 'Hello' }] } },
  { type: 'result', subtype: 'success', num_turns: 1, model: 'gpt-5' },
]

const strip = (entries) => entries.map(({ kind, name, text, result: r, input }) =>
  kind === 'tool' ? { kind, name, input, result: r } : kind === 'text' || kind === 'thinking' ? { kind, text } : { kind })

test('a Codex turn renders like a Claude one', () => {
  const state = createStreamState()
  const index = codexTurn.indexOf(codexTurn.find((e) => e.event?.delta?.text === 'lo'))
  for (const e of codexTurn.slice(0, index + 1)) applyStreamEvent(state, e)
  // Mid-answer: the text streams.
  assert.deepEqual(state.entries.at(-1), { kind: 'text', text: 'Hello', streaming: true })
  for (const e of codexTurn.slice(index + 1)) applyStreamEvent(state, e)

  assert.deepEqual(strip(state.entries.filter((e) => e.kind !== 'result')), [
    { kind: 'system' },
    { kind: 'thinking', text: 'Look around' },
    { kind: 'tool', name: 'Bash', input: { command: 'ls', description: 'ls' }, result: { content: 'a.txt\n', is_error: false } },
    { kind: 'tool', name: 'mcp__puppeteer__session_spawn', input: { title: 'worker' }, result: { content: 'spawned', is_error: false } },
    { kind: 'tool', name: 'Task', input: { subagent_type: 'spawnAgent', description: 'spawnAgent · gpt-5 · t2', prompt: 'Check the tests' }, result: { content: 't2: completed — All green', is_error: false } },
    { kind: 'tool', name: 'WebSearch', input: { query: 'codex app-server' }, result: { content: 'Searched the web for "codex app-server"', is_error: false } },
    { kind: 'tool', name: 'TodoWrite', input: { todos }, result: { content: 'Plan updated.', is_error: false } },
    { kind: 'tool', name: 'ExitPlanMode', input: { plan: '# Plan' }, result: { content: '', is_error: false } },
    { kind: 'compact' },
    { kind: 'text', text: 'Hello' },
  ])
  assert.equal(state.entries.filter((e) => e.streaming).length, 0)
})

test('a tool card opened at item/started is running until its result', () => {
  const state = createStreamState()
  applyStreamEvent(state, use('c1', 'Bash', { command: 'sleep 9' }))
  assert.equal(state.entries[0].result, undefined)
  applyStreamEvent(state, result('c1', ''))
  assert.deepEqual(state.entries[0].result, { content: '', is_error: false })
})

test('a Codex reconnect shows as a retry until output resumes', () => {
  let status = null
  for (const e of codexTurn.slice(0, codexTurn.findIndex((x) => x.subtype === 'api_retry') + 1)) status = nextLiveStatus(status, e)
  assert.deepEqual(status, { kind: 'retry', attempt: 1, maxRetries: 5, delayMs: 0, errorStatus: 502 })
  status = nextLiveStatus(status, codexTurn.find((e) => e.event?.delta?.text === 'Hel'))
  assert.equal(status, null)
})

test('the saved log (no deltas) parses to the same entries', () => {
  const live = createStreamState()
  for (const e of codexTurn) applyStreamEvent(live, e)
  const saved = parseStreamLog(codexTurn.filter((e) => e.type !== 'stream_event').map((e) => JSON.stringify(e)).join('\n'))
  assert.deepEqual(strip(saved.entries), strip(live.entries))
})
