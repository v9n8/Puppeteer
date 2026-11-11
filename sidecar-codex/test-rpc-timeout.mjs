// Codex RPC timeouts and interrupt (AC-04): a request the app-server never
// answers fails after its timeout (with the app-server's last stderr lines),
// an app-server that exits fails the requests still waiting, and `interrupt`
// is a `turn/interrupt` REQUEST naming the turn in flight — nothing at all
// with no turn running — and never takes the sidecar down.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { existsSync, mkdtempSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-app-server.mjs')

// `onMessage(msg, child)` sees every sidecar stdout message; resolves on exit.
function runSidecar(mode, onMessage = () => {}) {
  const work = mkdtempSync(join(tmpdir(), `puppeteer-codex-rpc-${mode}-`))
  const log = join(work, 'fixture.jsonl')
  const childEnv = { ...process.env }
  for (const k of ['PUPPETEER_RESUME_SESSION', 'PUPPETEER_ROLLBACK_TURNS', 'PUPPETEER_CODEX_KEEP_OPEN', 'PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS', 'PUPPETEER_CODEX_SEED_FRAMING', 'PUPPETEER_CODEX_MCP', 'PUPPETEER_FIXTURE_USAGE']) delete childEnv[k]
  return new Promise((resolve) => {
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: {
        ...childEnv,
        PUPPETEER_PERMISSION_MODE: 'default',
        PUPPETEER_CODEX_PATH: fixture,
        PUPPETEER_FIXTURE_RPC: mode,
        PUPPETEER_FIXTURE_LOG: log,
        // 30s → 0.3s, 120s → 1.2s
        PUPPETEER_CODEX_RPC_TIMEOUT_SCALE: '0.01',
      },
    })
    const out = []
    let buf = ''
    child.stdout.on('data', (d) => {
      buf += d
      let nl
      while ((nl = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, nl); buf = buf.slice(nl + 1)
        let msg
        try { msg = JSON.parse(line) } catch { continue }
        out.push(msg)
        onMessage(msg, child)
      }
    })
    // A hang fails the test instead of blocking CI.
    const timer = setTimeout(() => { try { child.kill('SIGKILL') } catch {} }, 8000)
    child.on('exit', (code) => {
      clearTimeout(timer)
      const rpc = existsSync(log)
        ? readFileSync(log, 'utf8').trim().split('\n').map((l) => JSON.parse(l)).filter((e) => e.method)
        : []
      resolve({
        code,
        out,
        rpc,
        errors: out.filter((m) => m.type === '_puppeteer_error').map((m) => m.error),
        warns: out.filter((m) => m.type === '_puppeteer_log' && m.level === 'warn').map((m) => m.text),
        closed: out.find((m) => m.type === '_puppeteer_closed') ?? null,
        results: out.filter((m) => m.type === 'result'),
      })
    })
  })
}

const send = (child, cmd) => child.stdin.write(JSON.stringify(cmd) + '\n')

// 1. initialize never answered: a timeout error carrying the stderr tail, and
//    the sidecar stops with a failure.
const init = await runSidecar('silent-initialize')
const initOk = init.code !== 0 && init.errors.length === 1 &&
  /initialize timed out after 0\.3s/.test(init.errors[0]) &&
  /Last app-server stderr:\nfixture: mode silent-initialize/.test(init.errors[0]) &&
  !init.out.some((m) => m.type === '_puppeteer_ready')

// 2. The app-server exits during thread/start: that request fails at once
//    (well before its 1.2s timeout), and the exit is reported as it happened.
const exited = await runSidecar('exit-on-thread-start')
const exitedOk = exited.code === 3 && exited.errors.length === 1 &&
  /app-server exited \(code 3\)/.test(exited.errors[0]) && !/timed out/.test(exited.errors[0]) &&
  exited.closed?.code === 3 && exited.closed?.signal === null

// 3. turn/start never answered: the turn is refused after its timeout (no
//    hang, no crash); the session then ends normally when stdin closes.
const turn = await runSidecar('silent-turn-start', (msg, child) => {
  if (msg.type === '_puppeteer_ready') send(child, { type: 'prompt', text: 'go' })
  if (msg.type === '_puppeteer_turn_failed') child.stdin.end()
})
const turnOk = turn.code === 0 && turn.errors.length === 1 &&
  /^turn\/start failed: turn\/start timed out after 0\.3s/.test(turn.errors[0]) &&
  turn.out.some((m) => m.type === '_puppeteer_turn_failed')

// 4. Interrupt during a turn: one turn/interrupt REQUEST (it has an id) naming
//    the thread and the turn; the turn then ends as interrupted.
const stop = await runSidecar('long-turn', (msg, child) => {
  if (msg.type === '_puppeteer_ready') { send(child, { type: 'prompt', text: 'go' }); send(child, { type: 'interrupt' }) }
})
const interrupts = stop.rpc.filter((e) => e.method === 'turn/interrupt')
const stopOk = stop.code === 0 && stop.errors.length === 0 && stop.warns.length === 0 &&
  interrupts.length === 1 && interrupts[0].id != null &&
  JSON.stringify(interrupts[0].params) === JSON.stringify({ threadId: 'fixture-thread', turnId: 'fixture-turn' }) &&
  stop.results.length === 1 && stop.results[0].subtype === 'interrupted'

// 5. Interrupt with no turn running: nothing is sent, and the sidecar stays up
//    until stdin closes (it used to crash on a TypeError).
const idle = await runSidecar('long-turn', (msg, child) => {
  if (msg.type === '_puppeteer_ready') {
    send(child, { type: 'interrupt' })
    setTimeout(() => child.stdin.end(), 300)
  }
})
const idleOk = idle.code === 0 && idle.errors.length === 0 &&
  !idle.rpc.some((e) => e.method === 'turn/interrupt') &&
  idle.closed?.code === 0

const pass = !!(initOk && exitedOk && turnOk && stopOk && idleOk)
console.log(JSON.stringify({
  initOk, exitedOk, turnOk, stopOk, idleOk,
  codes: [init, exited, turn, stop, idle].map((r) => r.code),
  errors: [init, exited, turn, stop, idle].map((r) => r.errors),
  pass,
}))
process.exit(pass ? 0 : 1)
