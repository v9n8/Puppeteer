// A normal Codex session loads and calls only the MCP servers Puppeteer passed
// (mcp-guard.mjs): every server of codex's own config that was not passed is
// turned off before start, ChatGPT apps and plugins are off, the session
// refuses to start when that config cannot be listed, and a call to a server
// that was not passed is reported and interrupts the turn.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync, existsSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import assert from 'node:assert/strict'
import { disableServerArgs, forwardedServerNames, mcpCallGuard } from './mcp-guard.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-app-server.mjs')
const PMS = { pms: { type: 'stdio', command: 'pms-server' } }

// One session on the fixture: `extra` env on top. Once ready it sends a prompt
// and closes input after the result (`prompt`), or closes input at once. Resolves with its messages, exit code and the
// fixture's log (argv of app-server + every client request).
function run(extra, { prompt = false } = {}) {
  const work = mkdtempSync(join(tmpdir(), 'puppeteer-codex-mcp-'))
  const log = join(work, 'fixture.jsonl')
  const env = { ...process.env, PUPPETEER_PERMISSION_MODE: 'bypassPermissions', PUPPETEER_CODEX_PATH: fixture, PUPPETEER_FIXTURE_LOG: log, ...extra }
  for (const k of ['PUPPETEER_RESUME_SESSION', 'PUPPETEER_CODEX_NO_TOOLS', 'PUPPETEER_FIXTURE_USAGE', 'PUPPETEER_FIXTURE_RPC']) {
    if (!(k in extra)) delete env[k]
  }
  if (!('PUPPETEER_CODEX_MCP' in extra)) delete env.PUPPETEER_CODEX_MCP
  return new Promise((resolve) => {
    const child = spawn('node', [join(here, 'index.mjs')], { cwd: work, stdio: ['pipe', 'pipe', 'pipe'], env })
    const messages = []
    let buf = ''
    child.stdout.on('data', (d) => {
      buf += d
      let nl
      while ((nl = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, nl); buf = buf.slice(nl + 1)
        let msg
        try { msg = JSON.parse(line) } catch { continue }
        messages.push(msg)
        if (msg.type === '_puppeteer_ready') {
          if (prompt) child.stdin.write(JSON.stringify({ type: 'prompt', text: 'go' }) + '\n')
          else child.stdin.end()
        }
        if (msg.type === 'result') setTimeout(() => child.stdin.end(), 100)
      }
    })
    child.stdin.on('error', () => {})
    const timer = setTimeout(() => child.kill('SIGKILL'), 10_000)
    child.on('exit', (code) => {
      clearTimeout(timer)
      const entries = existsSync(log) ? readFileSync(log, 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l)) : []
      resolve({ code, messages, argv: entries.find((e) => e.argv)?.argv ?? null, requests: entries.filter((e) => e.method) })
    })
  })
}

const flags = (argv) => argv.filter((a, i) => argv[i - 1] === '-c')
const blocked = (messages) => messages.filter((m) => m.type === '_puppeteer_mcp_blocked').map(({ server, source, reason }) => ({ server, source, reason }))

// The pure parts.
assert.deepEqual(disableServerArgs(['user_server', 'pms', 'bad.name', 'with space'], new Set(['pms'])), ['-c', 'mcp_servers.user_server.enabled=false'])
assert.deepEqual([...forwardedServerNames(['-c', 'mcp_servers.pms.tool_timeout_sec=60', '-c', 'mcp_servers.pms.command="x"', '-c', 'mcp_servers.web.url="https://h"', '-c', 'mcp_servers.half.args=["a"]'])], ['pms', 'web'])
{
  const reports = []
  let interrupts = 0
  const guard = mcpCallGuard(new Set(['pms']), { report: (b) => reports.push(b), interrupt: () => interrupts++ })
  const call = (id, server) => ({ item: { id, type: 'mcpToolCall', server, tool: 't' } })
  guard('item/started', call('m1', 'user_server'))
  guard('item/completed', call('m1', 'user_server'))
  guard('item/started', call('m2', 'pms'))
  guard('item/started', call('m3', 'other'))
  guard('item/started', { item: { id: 'c1', type: 'commandExecution' } })
  guard('mcpServer/startupStatus/updated', { name: 'policy', status: 'starting' })
  guard('mcpServer/startupStatus/updated', { name: 'policy', status: 'ready' })
  guard('mcpServer/startupStatus/updated', { name: 'pms', status: 'starting' })
  assert.deepEqual(reports, [
    { server: 'user_server', source: 'codex', reason: 'call' },
    { server: 'other', source: 'codex', reason: 'call' },
    { server: 'policy', source: 'codex', reason: 'loaded' },
  ])
  assert.equal(interrupts, 2, 'each blocked call interrupts once')
}
console.log('ok   pure parts: disable args, forwarded names, call guard')

// Before start: codex's own server is off, the passed one is forwarded, apps and plugins off.
{
  const r = await run({ PUPPETEER_CODEX_MCP: JSON.stringify(PMS) })
  assert.equal(r.code, 0, JSON.stringify(r.messages.filter((m) => m.type === '_puppeteer_error')))
  const f = flags(r.argv)
  assert.ok(f.includes('mcp_servers.user_server.enabled=false'), f.join(' '))
  assert.ok(f.includes('mcp_servers.pms.command="pms-server"'), f.join(' '))
  assert.ok(!f.includes('mcp_servers.pms.enabled=false'))
  assert.ok(f.includes('features.apps=false') && f.includes('features.plugins=false'), f.join(' '))
  assert.equal(r.argv.at(-1), 'app-server')
  assert.deepEqual(blocked(r.messages), [])
}
// No server passed: codex's own are still off.
{
  const r = await run({})
  assert.equal(r.code, 0)
  assert.ok(flags(r.argv).includes('mcp_servers.user_server.enabled=false'))
}
// A passed server named like one of codex's own stays on (the passed config wins).
{
  const r = await run({ PUPPETEER_CODEX_MCP: JSON.stringify({ user_server: { type: 'stdio', command: 'mine' } }) })
  assert.ok(!flags(r.argv).includes('mcp_servers.user_server.enabled=false'))
}
console.log('ok   normal session turns off every codex server it was not passed, plus apps and plugins')

// Fail-closed: codex's list unreadable → no session.
for (const mode of ['fail', 'garbage', 'object']) {
  const r = await run({ PUPPETEER_CODEX_MCP: JSON.stringify(PMS), PUPPETEER_FIXTURE_MCP_LIST: mode })
  const err = r.messages.find((m) => m.type === '_puppeteer_error')
  assert.ok(err && /^Refusing to start a codex session/.test(err.error), `${mode}: ${JSON.stringify(r.messages)}`)
  assert.equal(r.code, 1, `${mode}: exit code`)
  assert.equal(r.argv, null, `${mode}: codex app-server was started`)
}
console.log('ok   normal session refuses to start when codex servers cannot be listed')

// Runtime: a call to a server not passed is reported and interrupts the turn;
// a passed server's call goes on.
{
  const note = (method, params) => ({ method, params: { threadId: 'fixture-thread', turnId: 'fixture-turn', ...params } })
  const call = (id, server) => ({ id, type: 'mcpToolCall', server, tool: 'list', status: 'inProgress', arguments: {}, result: null, error: null })
  const script = [
    { method: 'mcpServer/startupStatus/updated', params: { name: 'policy_server', status: 'starting' } },
    note('item/started', { item: call('m1', 'pms') }),
    note('item/started', { item: call('m2', 'user_server') }),
    // The turn runs on until something interrupts it.
    { hold: true },
  ]
  const r = await run({ PUPPETEER_CODEX_MCP: JSON.stringify(PMS), PUPPETEER_FIXTURE_ITEMS: JSON.stringify(script) }, { prompt: true })
  assert.deepEqual(blocked(r.messages), [
    { server: 'policy_server', source: 'codex', reason: 'loaded' },
    { server: 'user_server', source: 'codex', reason: 'call' },
  ])
  const interrupts = r.requests.filter((e) => e.method === 'turn/interrupt')
  assert.equal(interrupts.length, 1, JSON.stringify(r.requests.map((e) => e.method)))
  assert.deepEqual(interrupts[0].params, { threadId: 'fixture-thread', turnId: 'fixture-turn' })
}
console.log('ok   a call to a server not passed is reported and interrupts the turn')
