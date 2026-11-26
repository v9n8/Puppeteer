#!/usr/bin/env node
// Deterministic tests of the MCP probe (mcp-probe.mjs): what it prints for each
// server the CLI reports, and that it never prints a launch value. No CLI, no
// network: the Agent SDK is replaced by test-fixtures/mock-sdk.mjs.
//
// Run: node test-mcp-probe.mjs   (from the sidecar/ dir)

import { spawn } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { dirname, join } from 'node:path'
import assert from 'node:assert/strict'
import { probeLine, probeOptions } from './mcp-probe.mjs'

const here = dirname(fileURLToPath(import.meta.url))

// Run the probe on the mock SDK; resolves with its stdout lines and exit code.
function runProbe(env, args = []) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      ['--import', pathToFileURL(join(here, 'test-fixtures', 'mock-sdk-register.mjs')).href, join(here, 'mcp-probe.mjs'), ...args],
      { cwd: here, env: { ...process.env, CLAUDE_CONFIG_DIR: join(tmpdir(), 'mcp-probe-no-config'), PUPPETEER_MCP_PROBE_SETTLE_MS: '10', ...env }, stdio: ['ignore', 'pipe', 'inherit'] },
    )
    let out = ''
    const timer = setTimeout(() => {
      child.kill('SIGKILL')
      reject(new Error(`probe did not exit in time. Saw: ${out}`))
    }, 10_000)
    child.stdout.on('data', (chunk) => { out += chunk.toString() })
    child.on('exit', (code) => {
      clearTimeout(timer)
      resolve({ code, raw: out, lines: out.split('\n').filter(Boolean).map((l) => JSON.parse(l)) })
    })
  })
}

// What the CLI reports (SDK 0.3.319 `McpServerStatus` shape), with values the probe must drop.
const STATUS = [
  { name: 'claude.ai Slack', status: 'connected', source: 'claudeai', scope: 'claudeai', config: { type: 'claudeai-proxy', url: 'https://mcp.example/slack', id: 'conn_slack' }, tools: [{ name: 't' }] },
  { name: 'claude.ai Drive', status: 'needs-auth', source: 'claudeai', config: { type: 'claudeai-proxy', url: 'https://mcp.example/drive', id: 'conn_drive' } },
  { name: 'fs', status: 'connected', source: 'user', scope: 'user', config: { type: 'stdio', command: 'SECRET-COMMAND', args: ['SECRET-ARG'], env: { TOKEN: 'SECRET-ENV' } } },
  { name: 'remote', status: 'failed', source: 'plugin', error: 'SECRET-ERROR', config: { type: 'http', url: 'https://SECRET-URL', headers: { Authorization: 'SECRET-HEADER' } } },
  { name: 'corp', status: 'pending', source: 'managed', config: { type: 'sse', url: 'https://SECRET-MANAGED' } },
  { name: 'bad\nname', status: 'connected' },
  { status: 'connected', source: 'user' },
]

const tests = []
const test = (name, fn) => tests.push({ name, fn })

test('prints name, source, status and transport; connectors also id and url', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'mcp-probe-'))
  const optionsFile = join(dir, 'options.json')
  try {
    const { code, lines, raw } = await runProbe({ MOCK_MCP_STATUS: JSON.stringify(STATUS), MOCK_OPTIONS_FILE: optionsFile }, [dir])
    assert.equal(code, 0)
    assert.deepEqual(lines, [
      { type: 'server', name: 'claude.ai Slack', source: 'claudeai', status: 'connected', transport: 'claudeai-proxy', connector: { id: 'conn_slack', url: 'https://mcp.example/slack' } },
      { type: 'server', name: 'claude.ai Drive', source: 'claudeai', status: 'needs-auth', transport: 'claudeai-proxy', connector: { id: 'conn_drive', url: 'https://mcp.example/drive' } },
      { type: 'server', name: 'fs', source: 'user', status: 'connected', transport: 'stdio' },
      { type: 'server', name: 'remote', source: 'plugin', status: 'failed', transport: 'http' },
      { type: 'server', name: 'corp', source: 'managed', status: 'pending', transport: 'sse' },
      { type: 'server', name: 'bad name', source: 'unknown', status: 'connected', transport: null },
      { type: 'done', count: 6 },
    ])
    assert.ok(!raw.includes('SECRET-'), raw)
    // Connectors must still come through: no strict mode, no passed map. No settings source, so the
    // CLI does not start the user-scope servers; nothing is persisted; the cwd is the argument.
    const options = JSON.parse(readFileSync(optionsFile, 'utf8'))
    assert.equal(options.cwd, dir)
    assert.equal(options.persistSession, false)
    assert.equal(options.strictMcpConfig, undefined)
    assert.equal(options.mcpServers, undefined)
    assert.deepEqual(options.settingSources, [])
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test('a custom CLI path reaches the SDK like in a run', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'mcp-probe-'))
  const optionsFile = join(dir, 'options.json')
  try {
    const { code } = await runProbe({ MOCK_MCP_STATUS: '[]', MOCK_OPTIONS_FILE: optionsFile, PUPPETEER_CLAUDE_PATH: '/opt/claude' }, [dir])
    assert.equal(code, 0)
    assert.equal(JSON.parse(readFileSync(optionsFile, 'utf8')).pathToClaudeCodeExecutable, '/opt/claude')
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
})

test('the profile\'s enabled plugins come back as flag settings, nothing else of its settings', async () => {
  const configDir = mkdtempSync(join(tmpdir(), 'mcp-probe-config-'))
  const cwd = mkdtempSync(join(tmpdir(), 'mcp-probe-'))
  const optionsFile = join(cwd, 'options.json')
  try {
    writeFileSync(join(configDir, 'settings.json'), JSON.stringify({
      enabledPlugins: { 'lint@market': true, 'off@market': false },
      env: { TOKEN: 'SECRET-ENV' },
      hooks: { PreToolUse: [] },
    }))
    const { code } = await runProbe({ MOCK_MCP_STATUS: '[]', MOCK_OPTIONS_FILE: optionsFile, CLAUDE_CONFIG_DIR: configDir }, [cwd])
    assert.equal(code, 0)
    const options = JSON.parse(readFileSync(optionsFile, 'utf8'))
    assert.deepEqual(options.settingSources, [])
    assert.deepEqual(options.settings, { enabledPlugins: { 'lint@market': true, 'off@market': false } })
    // No settings file, a bad one, or no usable key: no flag settings at all.
    assert.equal(probeOptions(cwd, { CLAUDE_CONFIG_DIR: join(configDir, 'missing') }).settings, undefined)
    writeFileSync(join(configDir, 'settings.json'), '{ bad json')
    assert.equal(probeOptions(cwd, { CLAUDE_CONFIG_DIR: configDir }).settings, undefined)
    writeFileSync(join(configDir, 'settings.json'), JSON.stringify({ enabledPlugins: ['x'] }))
    assert.equal(probeOptions(cwd, { CLAUDE_CONFIG_DIR: configDir }).settings, undefined)
  } finally {
    rmSync(configDir, { recursive: true, force: true })
    rmSync(cwd, { recursive: true, force: true })
  }
})

test('a failing status call is one error line and exit 1', async () => {
  const { code, lines } = await runProbe({ MOCK_MCP_STATUS: 'fail' })
  assert.equal(code, 1)
  assert.deepEqual(lines, [{ type: 'error', error: 'mock status failed' }])
})

test('no answer in time is one error line and exit 1', async () => {
  // The mock without MOCK_MCP_STATUS has no mcpServerStatus(): make the settle outlast the limit instead.
  const { code, lines } = await runProbe({ MOCK_MCP_STATUS: '[]', PUPPETEER_MCP_PROBE_SETTLE_MS: '5000', PUPPETEER_MCP_PROBE_TIMEOUT_MS: '100' })
  assert.equal(code, 1)
  assert.deepEqual(lines, [{ type: 'error', error: 'no answer within 100 ms' }])
})

test('probeLine keeps a connector only with a string id and url', () => {
  assert.deepEqual(probeLine({ name: 'x', source: 'claudeai', status: 'connected', config: { type: 'claudeai-proxy', id: 7, url: 'u' } }), {
    type: 'server', name: 'x', source: 'claudeai', status: 'connected', transport: 'claudeai-proxy',
  })
  assert.equal(probeLine({ name: 'y'.repeat(500) }).name.length, 200)
})

let failed = 0
for (const t of tests) {
  try {
    await t.fn()
    console.log(`ok   ${t.name}`)
  } catch (err) {
    failed++
    console.log(`FAIL ${t.name}\n${err.stack}`)
  }
}
console.log(failed ? `${failed} of ${tests.length} failed` : `all ${tests.length} passed`)
process.exit(failed ? 1 : 0)
