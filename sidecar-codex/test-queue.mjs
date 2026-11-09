// Follow-ups on Codex (D4-15): a prompt that arrives while a turn is in flight
// never starts a second turn over it (no turn/start until turn/completed) and
// is not lost when that turn ends. With PUPPETEER_CODEX_KEEP_OPEN the session
// outlives a turn until the broker closes stdin; a refused turn/start says so
// (`_puppeteer_turn_failed`) and the next prompt still runs.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync, readFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-framing.mjs')

// `onMessage(msg, child, out)` drives the sidecar; resolves once it exits.
function runSidecar(name, env, onMessage) {
  const work = mkdtempSync(join(tmpdir(), `puppeteer-codex-queue-${name}-`))
  const rpcOut = join(work, 'rpc.ndjson')
  const childEnv = { ...process.env }
  for (const k of ['PUPPETEER_RESUME_SESSION', 'PUPPETEER_ROLLBACK_TURNS', 'PUPPETEER_FIXTURE_STEER_ERROR', 'PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS', 'PUPPETEER_CODEX_SEED_FRAMING', 'PUPPETEER_CODEX_MCP', 'PUPPETEER_CODEX_KEEP_OPEN', 'PUPPETEER_FIXTURE_COMPLETE_MS', 'PUPPETEER_FIXTURE_FAIL_FIRST_TURN']) delete childEnv[k]
  return new Promise((resolve) => {
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...childEnv, PUPPETEER_PERMISSION_MODE: 'default', PUPPETEER_CODEX_PATH: fixture, PUPPETEER_FRAMING_RPC_OUT: rpcOut, PUPPETEER_FIXTURE_COMPLETE_MS: '200', ...env },
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
        onMessage(msg, child, out)
      }
    })
    // A hang fails the test instead of blocking CI.
    const timer = setTimeout(() => { try { child.kill('SIGKILL') } catch {} }, 8000)
    child.on('exit', (code) => {
      clearTimeout(timer)
      const rpc = existsSync(rpcOut)
        ? readFileSync(rpcOut, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l))
        : []
      resolve({ code, out, rpc, methods: rpc.map((r) => r.method) })
    })
  })
}

const send = (child, c) => child.stdin.write(JSON.stringify(c) + '\n')
const texts = (rpc) => rpc.filter((r) => r.method === 'turn/start').map((r) => r.params.input[0].text)
const types = (out, type) => out.filter((m) => m.type === type)

// 1. Two prompts at once: the second waits for the first turn to complete, then
//    runs; without KEEP_OPEN the session ends after the last one.
const a = await runSidecar('waits', {}, (msg, child) => {
  if (msg.type === '_puppeteer_ready') {
    send(child, { type: 'prompt', text: 'first' })
    send(child, { type: 'prompt', text: 'second' })
  }
})
const waitsOk = a.methods.join(',') === 'thread/start,turn/start,turn/completed,turn/start,turn/completed' &&
  texts(a.rpc).join(',') === 'first,second' &&
  types(a.out, 'result').length === 2 &&
  types(a.out, '_puppeteer_closed').at(-1)?.code === 0 && a.code === 0

// 2. KEEP_OPEN: the session outlives the turn; a prompt handed over after its
//    `result` still runs; closing stdin then ends it cleanly.
let results = 0
const b = await runSidecar('keep-open', { PUPPETEER_CODEX_KEEP_OPEN: '1' }, (msg, child) => {
  if (msg.type === '_puppeteer_ready') send(child, { type: 'prompt', text: 'first' })
  if (msg.type === 'result' && ++results === 1) setTimeout(() => send(child, { type: 'prompt', text: 'later' }), 300)
  if (msg.type === 'result' && results === 2) child.stdin.end()
})
const keepOpenOk = texts(b.rpc).join(',') === 'first,later' &&
  types(b.out, 'result').length === 2 &&
  types(b.out, '_puppeteer_closed').at(-1)?.code === 0 && b.code === 0

// 3. A refused turn/start: `_puppeteer_turn_failed` (no result follows), and
//    the prompt waiting behind it still starts.
const c = await runSidecar('refused', { PUPPETEER_CODEX_KEEP_OPEN: '1', PUPPETEER_FIXTURE_FAIL_FIRST_TURN: '1' }, (msg, child) => {
  if (msg.type === '_puppeteer_ready') {
    send(child, { type: 'prompt', text: 'first' })
    send(child, { type: 'prompt', text: 'second' })
  }
  if (msg.type === 'result') child.stdin.end()
})
const refusedOk = texts(c.rpc).join(',') === 'first,second' &&
  types(c.out, '_puppeteer_turn_failed').length === 1 &&
  types(c.out, 'result').length === 1 &&
  c.out.findIndex((m) => m.type === '_puppeteer_turn_failed') < c.out.findIndex((m) => m.type === 'result') &&
  c.code === 0

const pass = !!(waitsOk && keepOpenOk && refusedOk)
console.log(JSON.stringify({ waitsOk, keepOpenOk, refusedOk, methods: [a.methods, b.methods, c.methods], codes: [a.code, b.code, c.code], pass }))
process.exit(pass ? 0 : 1)
