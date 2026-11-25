#!/usr/bin/env node
// Deterministic tests of the MCP lockdown (mcp-guard.mjs): a Claude run loads and
// calls only the MCP servers Puppeteer passed. No CLI, no network: the Agent SDK
// is replaced by test-fixtures/mock-sdk.mjs (scenario `mcp-calls`).
//
// Run: node test-mcp-guard.mjs   (from the sidecar/ dir)

import { spawn } from 'node:child_process'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { dirname, join } from 'node:path'
import assert from 'node:assert/strict'
import { applyMcpLockdown, mcpCallVerdict, MCP_BLOCKED_REASON } from './mcp-guard.mjs'

const here = dirname(fileURLToPath(import.meta.url))

// Spawn the sidecar on the mock SDK; `onMessage(msg, ctl)` drives it. Resolves
// with every message once the process exits (rejects on a hang).
function runSidecar(scenario, onMessage, extraEnv = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      ['--import', pathToFileURL(join(here, 'test-fixtures', 'mock-sdk-register.mjs')).href, join(here, 'index.mjs')],
      { cwd: here, env: { ...process.env, MOCK_SCENARIO: scenario, ...extraEnv }, stdio: ['pipe', 'pipe', 'inherit'] },
    )
    const messages = []
    const ctl = {
      send: (obj) => child.stdin.write(JSON.stringify(obj) + '\n'),
      closeInput: () => child.stdin.end(),
    }
    const timer = setTimeout(() => {
      child.kill('SIGKILL')
      reject(new Error(`[${scenario}] sidecar did not exit in time. Saw: ${messages.map((m) => m.type).join(', ')}`))
    }, 10_000)
    let buf = ''
    child.stdout.on('data', (chunk) => {
      buf += chunk.toString()
      let i
      while ((i = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, i)
        buf = buf.slice(i + 1)
        if (!line.trim()) continue
        const msg = JSON.parse(line)
        messages.push(msg)
        onMessage(msg, ctl)
      }
    })
    child.on('exit', (code) => {
      clearTimeout(timer)
      resolve({ messages, code })
    })
  })
}

const of = (messages, type) => messages.filter((m) => m.type === type)
const SLACK = { tool: 'mcp__claude_ai_Slack__slack_send_message', server: { name: 'claude_ai_Slack', source: 'claudeai' } }
const PMS = { tool: 'mcp__pms__list', server: { name: 'pms', source: 'dynamic' } }
const PASSED = { pms: { type: 'stdio', command: 'pms-server' } }

// One session on the `mcp-calls` scenario: `calls` go through the hooks and the
// permission callback. Permission prompts are answered "allow".
async function session(options, calls, env = {}) {
  const { messages, code } = await runSidecar('mcp-calls', (msg, ctl) => {
    if (msg.type === '_puppeteer_ready') ctl.send({ type: 'prompt', text: 'go', options })
    if (msg.type === '_puppeteer_permission_request') ctl.send({ type: 'permission_response', requestId: msg.requestId, decision: 'allow' })
    if (msg.type === 'result') ctl.closeInput()
  }, { MOCK_MCP_CALLS: JSON.stringify(calls), ...env })
  assert.equal(code, 0)
  const init = messages.find((m) => m.type === 'system' && m.subtype === 'init')
  return {
    options: init.mock_options,
    calls: of(messages, 'result')[0].mock_calls,
    blocked: of(messages, '_puppeteer_mcp_blocked').map(({ server, source, reason }) => ({ server, source, reason })),
  }
}

const tests = {
  // The rule itself: only a passed server (by source and name) is let through.
  'verdict: only a passed server answers'() {
    const allowed = new Set(['pms'])
    assert.equal(mcpCallVerdict('Bash', undefined, allowed), null, 'not an MCP tool')
    assert.equal(mcpCallVerdict(PMS.tool, PMS.server, allowed), null)
    assert.equal(mcpCallVerdict('mcp__pms__x', { name: 'pms', source: 'sdk' }, allowed), null)
    assert.deepEqual(mcpCallVerdict(SLACK.tool, SLACK.server, allowed), { server: 'claude_ai_Slack', source: 'claudeai' })
    // A config server named like a passed one is still not the passed one.
    for (const source of ['user', 'project', 'local', 'plugin', 'managed', 'enterprise', 'claudeai', 'agent', 'something-new']) {
      assert.deepEqual(mcpCallVerdict('mcp__pms__x', { name: 'pms', source }, allowed), { server: 'pms', source }, source)
    }
    // A passed source with a name not passed.
    assert.deepEqual(mcpCallVerdict('mcp__other__x', { name: 'other', source: 'dynamic' }, allowed), { server: 'other', source: 'dynamic' })
    // No provenance (an older CLI) on an MCP tool: denied, server read from the name.
    assert.deepEqual(mcpCallVerdict('mcp__pms__list', undefined, allowed), { server: 'pms', source: 'unknown' })
    assert.deepEqual(mcpCallVerdict('mcp__pms__list', { name: 'pms' }, allowed), { server: 'pms', source: 'unknown' })
  },

  // The options: strict always, the hook added beside existing ones, the
  // permission callback wrapped (not added when there is none).
  async 'lockdown sets strict, appends the hook and wraps the callback'() {
    const sent = []
    const inner = []
    const routing = { matcher: 'Agent|Task', hooks: [async () => ({})] }
    const options = {
      mcpServers: { ...PASSED },
      hooks: { PreToolUse: [routing], Stop: [] },
      canUseTool: async (name) => { inner.push(name); return { behavior: 'allow' } },
    }
    applyMcpLockdown(options, (m) => sent.push(m))
    assert.equal(options.strictMcpConfig, true)
    assert.deepEqual(options.mcpServers, PASSED)
    assert.equal(options.hooks.PreToolUse[0], routing)
    assert.equal(options.hooks.PreToolUse[1].matcher, 'mcp__.*')
    assert.deepEqual(options.hooks.Stop, [])

    const denied = await options.canUseTool(SLACK.tool, {}, { mcpServer: SLACK.server })
    assert.deepEqual(denied, { behavior: 'deny', message: MCP_BLOCKED_REASON })
    assert.equal((await options.canUseTool(PMS.tool, {}, { mcpServer: PMS.server })).behavior, 'allow')
    assert.equal((await options.canUseTool('Write', {}, {})).behavior, 'allow')
    assert.deepEqual(inner, [PMS.tool, 'Write'], 'a denied call never reaches the permission prompt')
    assert.deepEqual(sent, [{ type: '_puppeteer_mcp_blocked', server: 'claude_ai_Slack', source: 'claudeai', reason: 'call' }])

    const bare = { permissionMode: 'bypassPermissions' }
    applyMcpLockdown(bare, () => {})
    assert.equal(bare.canUseTool, undefined)
    assert.equal(bare.strictMcpConfig, true)
    assert.equal(bare.hooks.PreToolUse.length, 1)
  },

  // Init: each server the CLI started beyond the passed ones is reported once.
  'init reports servers loaded beyond the passed ones, once'() {
    const sent = []
    const onMessage = applyMcpLockdown({ mcpServers: { ...PASSED } }, (m) => sent.push(m))
    const init = {
      type: 'system',
      subtype: 'init',
      mcp_servers: [
        { name: 'pms', status: 'connected', source: 'dynamic' },
        { name: 'policy', status: 'connected', source: 'managed' },
        { name: 'pms', status: 'connected', source: 'user' },
        { name: 'old-cli', status: 'connected' },
      ],
    }
    onMessage(init)
    onMessage(init)
    onMessage({ type: 'assistant' })
    assert.deepEqual(sent.map(({ server, source, reason }) => [server, source, reason]), [
      ['policy', 'managed', 'loaded'],
      ['pms', 'user', 'loaded'],
      ['old-cli', 'unknown', 'loaded'],
    ])
  },

  // Through the sidecar, in bypass mode (no permission callback): the hook
  // alone denies, and the app is told.
  async 'a run denies claude.ai, unknown and unsourced MCP tools; a passed server works'() {
    const s = await session(
      { permissionMode: 'bypassPermissions', mcpServers: PASSED },
      [SLACK, PMS, { tool: 'mcp__odd__x', server: { name: 'odd', source: 'something-new' } }, { tool: 'mcp__pms__list' }, { tool: 'Bash' }],
      { MOCK_MCP_INIT: JSON.stringify([{ name: 'pms', status: 'connected', source: 'dynamic' }, { name: 'claude.ai Slack', status: 'connected', source: 'claudeai' }]) },
    )
    assert.equal(s.options.strictMcpConfig, true)
    assert.deepEqual(s.options.mcpServers, PASSED)
    assert.deepEqual(s.calls.map((c) => [c.tool, c.by, c.behavior]), [
      [SLACK.tool, 'hook', 'deny'],
      [PMS.tool, 'none', 'allow'],
      ['mcp__odd__x', 'hook', 'deny'],
      ['mcp__pms__list', 'hook', 'deny'],
      ['Bash', 'none', 'allow'],
    ])
    assert.deepEqual(s.blocked, [
      { server: 'claude.ai Slack', source: 'claudeai', reason: 'loaded' },
      { server: 'claude_ai_Slack', source: 'claudeai', reason: 'call' },
      { server: 'odd', source: 'something-new', reason: 'call' },
      { server: 'pms', source: 'unknown', reason: 'call' },
    ])
  },

  // A gated mode keeps its permission prompt for a passed server.
  async 'a gated run still asks for a passed server and never for a blocked one'() {
    const s = await session({ permissionMode: 'default', mcpServers: PASSED }, [SLACK, PMS])
    assert.deepEqual(s.calls.map((c) => [c.tool, c.by, c.behavior]), [[SLACK.tool, 'hook', 'deny'], [PMS.tool, 'callback', 'allow']])
    assert.equal(s.blocked.length, 1)
  },

  // Resume: the servers come from PUPPETEER_MCP_SERVERS; they stay allowed.
  async 'a resumed run allows the servers of its env fallback'() {
    const s = await session({ permissionMode: 'bypassPermissions' }, [PMS, SLACK], { PUPPETEER_MCP_SERVERS: JSON.stringify(PASSED) })
    assert.deepEqual(s.options.mcpServers, PASSED)
    assert.deepEqual(s.calls.map((c) => c.behavior), ['allow', 'deny'])
  },

  // No servers passed: strict still on, every MCP tool denied.
  async 'a run without servers loads none'() {
    const s = await session({ permissionMode: 'bypassPermissions' }, [PMS])
    assert.equal(s.options.strictMcpConfig, true)
    assert.deepEqual(s.calls.map((c) => c.behavior), ['deny'])
  },

  // A side task keeps its own lockdown, untouched: no hook, deny-all callback.
  async 'a side task (noTools) is unchanged'() {
    const s = await session({ noTools: true, permissionMode: 'bypassPermissions', mcpServers: PASSED }, [SLACK])
    assert.equal(s.options.hooks, undefined)
    assert.deepEqual(s.options.mcpServers, {})
    assert.equal(s.options.strictMcpConfig, true)
    assert.deepEqual(s.calls.map((c) => [c.by, c.behavior]), [['callback', 'deny']])
    assert.deepEqual(s.blocked, [])
  },
}

let failed = 0
for (const [name, fn] of Object.entries(tests)) {
  try {
    await fn()
    console.log(`ok   ${name}`)
  } catch (e) {
    failed++
    console.log(`FAIL ${name}\n     ${e.message}`)
  }
}
console.log(failed ? `\n${failed} failed` : `\nall ${Object.keys(tests).length} passed`)
process.exit(failed ? 1 : 0)
