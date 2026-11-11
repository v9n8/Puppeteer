// Unit tests for the engine status events in lib/streamEvents.ts (AC-14):
// API retry, compaction, tool elapsed time and task progress; the switch to
// the fallback model (AC-17).
// Run: node --test src/lib/__tests__/streamEventsStatus.test.mjs
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

const retry = (attempt, extra = {}) => ({
  type: 'system', subtype: 'api_retry', attempt, max_retries: 10, retry_delay_ms: 8000, error_status: 529, ...extra,
})
const textDelta = (text, extra = {}) => ({
  type: 'stream_event',
  parent_tool_use_id: null,
  event: { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text } },
  ...extra,
})
const toolUse = (id, name, extra = {}) => ({
  type: 'assistant',
  parent_tool_use_id: null,
  message: { content: [{ type: 'tool_use', id, name, input: {} }] },
  ...extra,
})
const toolResult = (id) => ({
  type: 'user',
  message: { content: [{ type: 'tool_result', tool_use_id: id, content: 'ok' }] },
})
const heartbeat = (id, elapsed) => ({
  type: 'tool_progress', tool_name: 'Bash', tool_use_id: `${id}-heartbeat-${Math.floor(elapsed / 30)}`,
  parent_tool_use_id: id, elapsed_time_seconds: elapsed, heartbeat: true,
})

function fold(events, prev = null) {
  return events.reduce((s, e) => nextLiveStatus(s, e), prev)
}

test('api_retry: a retry line that the next main-thread token clears', () => {
  const s = fold([retry(2)])
  assert.deepEqual(s, { kind: 'retry', attempt: 2, maxRetries: 10, delayMs: 8000, errorStatus: 529 })
  assert.deepEqual(nextLiveStatus(s, retry(3, { error_status: null })), {
    kind: 'retry', attempt: 3, maxRetries: 10, delayMs: 8000, errorStatus: null,
  })
  assert.equal(nextLiveStatus(s, textDelta('Hi')), null)
  assert.equal(nextLiveStatus(s, toolUse('t1', 'Bash')), null)
})

test('api_retry: subagent output, tool progress and unrelated events keep it', () => {
  const s = fold([retry(2)])
  assert.equal(nextLiveStatus(s, textDelta('sub', { parent_tool_use_id: 'toolu_agent' })), s)
  assert.equal(nextLiveStatus(s, toolUse('t2', 'Read', { parent_tool_use_id: 'toolu_agent' })), s)
  assert.equal(nextLiveStatus(s, heartbeat('t1', 30)), s)
  assert.equal(nextLiveStatus(s, { type: 'system', subtype: 'status', status: 'requesting' }), s)
  assert.equal(nextLiveStatus(s, { type: 'rate_limit_event' }), s)
})

test('a turn ending or a new one starting drops any live status', () => {
  assert.equal(nextLiveStatus(fold([retry(1)]), { type: 'result', subtype: 'success' }), null)
  assert.equal(nextLiveStatus(fold([retry(1)]), { type: 'system', subtype: 'init' }), null)
  assert.equal(nextLiveStatus(null, { type: 'result' }), null)
})

test('status compacting → compact_boundary: the line clears and the divider gets its counts', () => {
  const compacting = { type: 'system', subtype: 'status', status: 'compacting' }
  const s = fold([compacting])
  assert.deepEqual(s, { kind: 'compacting' })
  assert.equal(nextLiveStatus(s, compacting), s, 'a repeated status keeps the same object')
  assert.equal(nextLiveStatus(s, { type: 'system', subtype: 'status', status: null }), null)

  const boundary = {
    type: 'system', subtype: 'compact_boundary', compact_metadata: { trigger: 'manual', pre_tokens: 157881, post_tokens: 18204 },
  }
  assert.equal(nextLiveStatus(s, boundary), null)

  const state = createStreamState()
  applyStreamEvent(state, compacting)
  assert.deepEqual(state.entries, [], 'the status itself adds no entry')
  applyStreamEvent(state, boundary)
  assert.deepEqual(state.entries, [{ kind: 'compact', summary: '', preTokens: 157881, postTokens: 18204 }])
  applyStreamEvent(state, {
    type: 'user', isCompactSummary: true,
    message: { content: 'This session is being continued from a previous conversation. Summary…' },
  })
  assert.deepEqual(state.entries, [{
    kind: 'compact',
    summary: 'This session is being continued from a previous conversation. Summary…',
    preTokens: 157881,
    postTokens: 18204,
  }], 'the summary fills the divider the boundary opened, not a second one')
})

test('compact_boundary: an older CLI without post_tokens, and a summary arriving first', () => {
  const state = createStreamState()
  applyStreamEvent(state, { type: 'user', isCompactSummary: true, message: { content: 'Summary A' } })
  applyStreamEvent(state, { type: 'system', subtype: 'compact_boundary', compact_metadata: { trigger: 'auto', pre_tokens: 1200 } })
  assert.deepEqual(state.entries, [{ kind: 'compact', summary: 'Summary A', preTokens: 1200 }])
})

test('a saved transcript (camelCase compactMetadata) parses to one divider', () => {
  const lines = [
    { type: 'system', subtype: 'init', model: 'claude-opus-4-7' },
    { type: 'system', subtype: 'compact_boundary', compactMetadata: { trigger: 'manual', preTokens: 5000, postTokens: 900 } },
    { type: 'user', isCompactSummary: true, message: { content: 'Summary B' } },
  ].map((l) => JSON.stringify(l)).join('\n')
  const state = parseStreamLog(lines)
  assert.deepEqual(state.entries.filter((e) => e.kind === 'compact'), [
    { kind: 'compact', summary: 'Summary B', preTokens: 5000, postTokens: 900 },
  ])
})

test('tool_progress: a heartbeat updates the elapsed time of its own tool only', () => {
  const state = createStreamState()
  applyStreamEvent(state, toolUse('toolu_a', 'Bash'))
  applyStreamEvent(state, toolUse('toolu_b', 'Bash'))
  const before = state.entries[1]
  applyStreamEvent(state, heartbeat('toolu_b', 42.7))
  assert.equal(state.entries[1].elapsedSecs, 42)
  assert.notEqual(state.entries[1], before, 'a new entry object, so the memoized card re-renders')
  assert.equal(state.entries[0].elapsedSecs, undefined)

  applyStreamEvent(state, heartbeat('toolu_b', 72))
  assert.equal(state.entries[1].elapsedSecs, 72)

  // Once the result is in, a late frame changes nothing.
  applyStreamEvent(state, toolResult('toolu_b'))
  const done = state.entries[1]
  applyStreamEvent(state, heartbeat('toolu_b', 102))
  assert.equal(state.entries[1], done)
  assert.equal(done.elapsedSecs, 72)
  assert.equal(done.result.content, 'ok')
})

test('tool_progress: a subagent frame names its own tool, never the subagent call', () => {
  const state = createStreamState()
  applyStreamEvent(state, toolUse('toolu_agent', 'Agent'))
  applyStreamEvent(state, toolUse('toolu_sub', 'Bash', { parent_tool_use_id: 'toolu_agent' }))
  applyStreamEvent(state, {
    type: 'tool_progress', tool_name: 'Bash', tool_use_id: 'toolu_sub', parent_tool_use_id: 'toolu_agent', elapsed_time_seconds: 15,
  })
  assert.equal(state.entries[1].elapsedSecs, 15)
  assert.equal(state.entries[0].elapsedSecs, undefined)
  // Unknown tool: ignored, and not pinned on the parent either.
  applyStreamEvent(state, {
    type: 'tool_progress', tool_name: 'Read', tool_use_id: 'toolu_gone', parent_tool_use_id: 'toolu_agent', elapsed_time_seconds: 20,
  })
  assert.equal(state.entries[0].elapsedSecs, undefined)
})

test('task_progress / task_notification: the subagent card follows its task', () => {
  const state = createStreamState()
  applyStreamEvent(state, toolUse('toolu_agent', 'Agent'))
  applyStreamEvent(state, {
    type: 'system', subtype: 'task_progress', task_id: 'a1', tool_use_id: 'toolu_agent', description: 'Explore',
    usage: { total_tokens: 12000, tool_uses: 3, duration_ms: 45000 }, last_tool_name: 'Grep',
  })
  assert.deepEqual(state.entries[0].task, {
    toolUses: 3, tokens: 12000, durationMs: 45000, lastTool: 'Grep', summary: undefined, done: false,
  })
  applyStreamEvent(state, {
    type: 'system', subtype: 'task_notification', task_id: 'a1', tool_use_id: 'toolu_agent', status: 'completed',
    output_file: '', summary: 'Found it', usage: { total_tokens: 20000, tool_uses: 5, duration_ms: 60000 },
  })
  assert.deepEqual(state.entries[0].task, {
    toolUses: 5, tokens: 20000, durationMs: 60000, lastTool: 'Grep', summary: 'Found it', done: true,
  })
  // A task with no matching tool call adds nothing.
  const n = state.entries.length
  applyStreamEvent(state, { type: 'system', subtype: 'task_progress', task_id: 'x', usage: { tool_uses: 1 } })
  assert.equal(state.entries.length, n)
})

test('model_fallback (AC-17): one warning entry naming both models, from the stream and a transcript', () => {
  const state = createStreamState()
  applyStreamEvent(state, {
    type: 'system', subtype: 'model_fallback', trigger: 'overloaded',
    original_model: 'claude-opus-5-5', fallback_model: 'claude-sonnet-4-6',
    content: 'Switched to Sonnet 4.6 due to high demand for Opus 5.5',
  })
  assert.deepEqual(state.entries, [{ kind: 'model-fallback', model: 'claude-opus-5-5', fallback: 'claude-sonnet-4-6' }])

  const saved = parseStreamLog(JSON.stringify({
    type: 'system', subtype: 'model_fallback', trigger: 'model_not_found', originalModel: 'claude-opus-5-5', fallbackModel: 'sonnet',
  }))
  assert.deepEqual(saved.entries, [{ kind: 'model-fallback', model: 'claude-opus-5-5', fallback: 'sonnet' }])

  // Without the fallback's name there is nothing to say.
  const bare = createStreamState()
  applyStreamEvent(bare, { type: 'system', subtype: 'model_fallback', original_model: 'claude-opus-5-5' })
  assert.deepEqual(bare.entries, [])
})
