// Codex streaming and items (AC-15): text and reasoning-summary deltas go out
// as Claude-shaped `stream_event`s, every item type becomes the Claude-shaped
// message the frontend renders (tool calls from item/started when their input
// is known), a plan update becomes TodoWrite, a compaction its boundary (once),
// a reconnect an api_retry status, and serverRequest/resolved withdraws the
// open prompt without answering it. The unused deltas are opted out at
// initialize.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import assert from 'node:assert/strict'

const here = dirname(fileURLToPath(import.meta.url))
const work = mkdtempSync(join(tmpdir(), 'puppeteer-codex-items-'))
const fixtureLog = join(work, 'fixture.log')

const T = 'fixture-thread'
const turnId = 'fixture-turn'
const note = (method, params) => ({ method, params: { threadId: T, turnId, ...params } })
const started = (item) => note('item/started', { item })
const completed = (item) => note('item/completed', { item })

const command = { id: 'c1', type: 'commandExecution', command: 'ls', commandActions: [{ type: 'listFiles', name: 'ls' }], status: 'inProgress', aggregatedOutput: null, exitCode: null }
const mcpCall = { id: 'm1', type: 'mcpToolCall', server: 'puppeteer', tool: 'session_spawn', status: 'inProgress', arguments: { title: 'worker' }, result: null, error: null }
const collab = { id: 'k1', type: 'collabAgentToolCall', tool: 'spawnAgent', status: 'inProgress', senderThreadId: T, receiverThreadIds: ['t2'], prompt: 'Check the tests', model: 'gpt-5', reasoningEffort: null, agentsStates: {} }

const script = [
  // Reasoning: summary deltas in two parts, the complete item without text.
  started({ id: 'r1', type: 'reasoning', summary: [], content: [] }),
  note('item/reasoning/summaryTextDelta', { itemId: 'r1', delta: 'Look', summaryIndex: 0 }),
  note('item/reasoning/summaryTextDelta', { itemId: 'r1', delta: ' around', summaryIndex: 0 }),
  note('item/reasoning/summaryTextDelta', { itemId: 'r1', delta: 'Then act', summaryIndex: 1 }),
  completed({ id: 'r1', type: 'reasoning', summary: [], content: [] }),
  // A command: its card from item/started, its result on item/completed.
  started(command),
  completed({ ...command, status: 'completed', aggregatedOutput: 'a.txt\n', exitCode: 0 }),
  // The conductor's own MCP tool, started then done.
  started(mcpCall),
  completed({ ...mcpCall, status: 'completed', result: { content: [{ type: 'text', text: 'spawned' }], structuredContent: null, _meta: null } }),
  // An MCP call seen only when it failed; a server name with a space.
  completed({ id: 'm2', type: 'mcpToolCall', server: 'my server', tool: 'fetch', status: 'failed', arguments: {}, result: null, error: { message: 'boom' } }),
  // A sub-agent.
  started(collab),
  completed({ ...collab, status: 'completed', agentsStates: { t2: { status: 'completed', message: 'All green' } } }),
  // A web search names its query only once done.
  started({ id: 'w1', type: 'webSearch', query: '', action: null }),
  completed({ id: 'w1', type: 'webSearch', query: 'codex app-server', action: { type: 'search', query: 'codex app-server', queries: null } }),
  note('turn/plan/updated', { explanation: null, plan: [{ step: 'Read the code', status: 'completed' }, { step: 'Write the test', status: 'inProgress' }, { step: 'Ship', status: 'pending' }] }),
  completed({ id: 'p1', type: 'plan', text: '# Plan\n1. Do it' }),
  // A compaction, reported by both its item and the deprecated notification.
  started({ id: 'x1', type: 'contextCompaction' }),
  completed({ id: 'x1', type: 'contextCompaction' }),
  note('thread/compacted', {}),
  completed({ id: 'i1', type: 'imageView', path: '/tmp/shot.png' }),
  completed({ id: 'd1', type: 'dynamicToolCall', namespace: null, tool: 'lookup', arguments: { q: 1 }, status: 'completed', contentItems: [{ type: 'inputText', text: 'hit' }], success: true, durationMs: 3 }),
  // An approval codex withdraws before anyone answered it.
  { id: 93, method: 'item/commandExecution/requestApproval', params: { threadId: T, turnId, itemId: 'c2', command: 'rm -rf build', reason: 'fixture approval' } },
  { method: 'serverRequest/resolved', params: { threadId: T, requestId: 93 } },
  // A reconnect codex retries on its own.
  note('error', { error: { message: 'Reconnecting... 1/5', codexErrorInfo: { responseStreamDisconnected: { httpStatusCode: 502 } }, additionalDetails: null }, willRetry: true }),
  // The answer streams, then arrives whole.
  started({ id: 'a1', type: 'agentMessage', text: '', phase: null, memoryCitation: null }),
  note('item/agentMessage/delta', { itemId: 'a1', delta: 'Hel' }),
  note('item/agentMessage/delta', { itemId: 'a1', delta: 'lo' }),
  // A sub-agent's delta (another thread) is not streamed here.
  { method: 'item/agentMessage/delta', params: { threadId: 't2', turnId: 'other', itemId: 'a9', delta: 'nope' } },
  completed({ id: 'a1', type: 'agentMessage', text: 'Hello', phase: null, memoryCitation: null }),
  // Never shown.
  completed({ id: 'u1', type: 'userMessage', content: [] }),
  completed({ id: 'g1', type: 'imageGeneration', status: 'completed', revisedPrompt: null, result: '' }),
]

const env = {
  ...process.env,
  PUPPETEER_PERMISSION_MODE: 'default',
  PUPPETEER_CODEX_PATH: join(here, 'test-fixture-app-server.mjs'),
  PUPPETEER_CODEX_KEEP_OPEN: '1',
  PUPPETEER_FIXTURE_ITEMS: JSON.stringify(script),
  PUPPETEER_FIXTURE_LOG: fixtureLog,
}
for (const k of ['PUPPETEER_RESUME_SESSION', 'PUPPETEER_ROLLBACK_TURNS', 'PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS', 'PUPPETEER_CODEX_SEED_FRAMING', 'PUPPETEER_CODEX_MCP', 'PUPPETEER_FIXTURE_USAGE', 'PUPPETEER_FIXTURE_RPC']) delete env[k]

const out = []
await new Promise((resolve) => {
  const child = spawn('node', [join(here, 'index.mjs')], { cwd: work, stdio: ['pipe', 'pipe', 'pipe'], env })
  let buf = ''
  child.stdout.on('data', (d) => {
    buf += d
    let nl
    while ((nl = buf.indexOf('\n')) >= 0) {
      const line = buf.slice(0, nl); buf = buf.slice(nl + 1)
      let msg
      try { msg = JSON.parse(line) } catch { continue }
      out.push(msg)
      if (msg.type === '_puppeteer_ready') child.stdin.write(JSON.stringify({ type: 'prompt', text: 'go' }) + '\n')
      // A late answer to the withdrawn prompt must go nowhere.
      if (msg.type === '_puppeteer_permission_cancelled') {
        child.stdin.write(JSON.stringify({ type: 'permission_response', requestId: msg.requestId, decision: 'allow' }) + '\n')
      }
      if (msg.type === 'result') setTimeout(() => child.stdin.end(), 200)
    }
  })
  const timer = setTimeout(() => { try { child.kill('SIGKILL') } catch {} }, 10_000)
  child.on('exit', () => { clearTimeout(timer); resolve() })
})

// One line per message the frontend acts on, in arrival order.
function trace(m) {
  switch (m.type) {
    case 'stream_event': {
      const ev = m.event
      if (ev.type === 'content_block_start') return `start ${ev.content_block.type}`
      return `delta ${ev.delta.type} ${JSON.stringify(ev.delta.text ?? ev.delta.thinking)}`
    }
    case 'assistant':
      return m.message.content.map((b) => b.type === 'tool_use' ? `use ${b.name} ${b.id}` : `${b.type} ${JSON.stringify(b.text ?? b.thinking)}`).join(' | ')
    case 'user':
      return m.message.content.map((b) => `result ${b.tool_use_id}${b.is_error ? ' error' : ''}`).join(' | ')
    case 'system':
      return m.subtype === 'init' ? null : `system ${m.subtype}${m.status ? ` ${m.status}` : ''}`
    case 'result':
      return 'result'
    case '_puppeteer_permission_request':
      return 'permission request'
    case '_puppeteer_permission_cancelled':
      return 'permission cancelled'
    default:
      return null
  }
}
const lines = out.map(trace).filter(Boolean)
const planId = out.find((m) => m.type === 'assistant' && m.message.content[0]?.name === 'TodoWrite')?.message.content[0].id

const checks = {}
function check(name, fn) {
  try { fn(); checks[name] = true } catch (e) { checks[name] = false; console.error(`${name}: ${e.message}`) }
}

check('order', () => assert.deepEqual(lines, [
  'start thinking',
  'delta thinking_delta "Look"',
  'delta thinking_delta " around"',
  'delta thinking_delta "\\nThen act"',
  'thinking "Look around\\nThen act"',
  'use Bash c1',
  'result c1',
  'use mcp__puppeteer__session_spawn m1',
  'result m1',
  'use mcp__my_server__fetch m2',
  'result m2 error',
  'use Task k1',
  'result k1',
  'use WebSearch w1',
  'result w1',
  `use TodoWrite ${planId}`,
  `result ${planId}`,
  'use ExitPlanMode p1',
  'result p1',
  'system status compacting',
  'system compact_boundary',
  'use Read i1',
  'result i1',
  'use lookup d1',
  'result d1',
  'permission request',
  'permission cancelled',
  'system api_retry',
  'start text',
  'delta text_delta "Hel"',
  'delta text_delta "lo"',
  'text "Hello"',
  'result',
]))

// The same shape as the Claude sidecar's partial messages.
check('streamShape', () => {
  const events = out.filter((m) => m.type === 'stream_event' && (m.event.delta?.type === 'text_delta' || m.event.content_block?.type === 'text'))
  assert.deepEqual(events[0], { type: 'stream_event', event: { type: 'content_block_start', index: 1, content_block: { type: 'text', text: '' } }, parent_tool_use_id: null })
  assert.deepEqual(events[1], { type: 'stream_event', event: { type: 'content_block_delta', index: 1, delta: { type: 'text_delta', text: 'Hel' } }, parent_tool_use_id: null })
})

const use = (id) => out.find((m) => m.type === 'assistant' && m.message.content[0]?.id === id)?.message.content[0]
const result = (id) => out.find((m) => m.type === 'user' && m.message.content[0]?.tool_use_id === id)?.message.content[0]

check('toolInputs', () => {
  assert.deepEqual(use('c1').input, { command: 'ls', description: 'ls' })
  assert.deepEqual(use('m1').input, { title: 'worker' })
  assert.deepEqual(use('k1').input, { subagent_type: 'spawnAgent', description: 'spawnAgent · gpt-5 · t2', prompt: 'Check the tests' })
  assert.deepEqual(use('w1').input, { query: 'codex app-server' })
  assert.deepEqual(use(planId).input, { todos: [
    { content: 'Read the code', status: 'completed', activeForm: 'Read the code' },
    { content: 'Write the test', status: 'in_progress', activeForm: 'Write the test' },
    { content: 'Ship', status: 'pending', activeForm: 'Ship' },
  ] })
  assert.deepEqual(use('p1').input, { plan: '# Plan\n1. Do it' })
  assert.deepEqual(use('i1').input, { file_path: '/tmp/shot.png' })
})

check('toolResults', () => {
  assert.equal(result('c1').content, 'a.txt\n')
  assert.deepEqual(result('m1').content, [{ type: 'text', text: 'spawned' }])
  assert.equal(result('m2').content, 'boom')
  assert.equal(result('k1').content, 't2: completed — All green')
  assert.equal(result('w1').content, 'Searched the web for "codex app-server"')
  assert.equal(result('d1').content, 'hit')
})

check('retryStatus', () => {
  const retry = out.find((m) => m.type === 'system' && m.subtype === 'api_retry')
  assert.deepEqual(retry, { type: 'system', subtype: 'api_retry', attempt: 1, max_retries: 5, retry_delay_ms: 0, error_status: 502, error: 'responseStreamDisconnected' })
  // The log line stays.
  assert.ok(out.some((m) => m.type === '_puppeteer_log' && m.text === '⚠ Codex: Reconnecting... 1/5'))
})

const logged = readFileSync(fixtureLog, 'utf8').trim().split('\n').map((l) => JSON.parse(l))
check('permissionWithdrawn', () => {
  const request = out.find((m) => m.type === '_puppeteer_permission_request')
  const cancelled = out.find((m) => m.type === '_puppeteer_permission_cancelled')
  assert.ok(request.requestId.endsWith('-93'))
  assert.equal(cancelled.requestId, request.requestId)
  assert.equal(cancelled.text, 'Permission request for Bash was withdrawn.')
  // Never answered: not by the late "allow", not when stdin closed.
  assert.ok(!logged.some((l) => l.id === 93 && !l.method), 'codex got an answer to the withdrawn request')
})

check('optOut', () => {
  const init = logged.find((l) => l.method === 'initialize')
  const optOut = init.params.capabilities.optOutNotificationMethods
  assert.ok(optOut.includes('item/commandExecution/outputDelta'))
  assert.ok(optOut.includes('item/reasoning/textDelta'))
  for (const used of ['item/agentMessage/delta', 'item/reasoning/summaryTextDelta', 'item/started', 'item/completed', 'serverRequest/resolved', 'turn/plan/updated', 'thread/compacted', 'error']) {
    assert.ok(!optOut.includes(used), `${used} is opted out`)
  }
})

check('noErrors', () => assert.deepEqual(out.filter((m) => m.type === '_puppeteer_error'), []))

const pass = Object.values(checks).every(Boolean)
console.log(JSON.stringify({ ...checks, pass }))
process.exit(pass ? 0 : 1)
