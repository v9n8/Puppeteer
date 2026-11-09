// Steer on Codex (D4-06): a steer goes to the turn in flight as `turn/steer`
// naming that turn — never as a new turn/start — even when it arrives while
// the session is still starting. With no turn to steer, or when codex refuses
// it, the sidecar warns instead of replaying it. Only a steer codex took is
// reported back (`_puppeteer_steered`), for the broker to log (R-13).
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync, readFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-framing.mjs')

function runSidecar(name, commands, env = {}) {
  const work = mkdtempSync(join(tmpdir(), `puppeteer-codex-steer-${name}-`))
  const rpcOut = join(work, 'rpc.ndjson')
  const childEnv = { ...process.env }
  for (const k of ['PUPPETEER_RESUME_SESSION', 'PUPPETEER_ROLLBACK_TURNS', 'PUPPETEER_FIXTURE_STEER_ERROR', 'PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS', 'PUPPETEER_CODEX_SEED_FRAMING', 'PUPPETEER_CODEX_MCP']) delete childEnv[k]
  return new Promise((resolve) => {
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...childEnv, PUPPETEER_PERMISSION_MODE: 'default', PUPPETEER_CODEX_PATH: fixture, PUPPETEER_FRAMING_RPC_OUT: rpcOut, ...env },
    })
    const out = []
    let buf = ''
    child.stdout.on('data', (d) => {
      buf += d
      let nl
      while ((nl = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, nl); buf = buf.slice(nl + 1)
        try { out.push(JSON.parse(line)) } catch {}
      }
    })
    // Written at once, before the sidecar is ready: the race a quick steer hits.
    for (const c of commands) child.stdin.write(JSON.stringify(c) + '\n')
    setTimeout(() => {
      try { child.kill() } catch {}
      const rpc = existsSync(rpcOut)
        ? readFileSync(rpcOut, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l))
        : []
      const warnings = out.filter((m) => m.type === '_puppeteer_log' && m.level === 'warn').map((m) => m.text)
      const steeredAcks = out.filter((m) => m.type === '_puppeteer_steered')
      resolve({ methods: rpc.map((r) => r.method), rpc, warnings, steeredAcks })
    }, 1500)
  })
}

const prompt = { type: 'prompt', text: 'build it' }
const steer = { type: 'steer', text: 'use tabs', images: [{ media_type: 'image/png', data: 'AAAA' }] }

const a = await runSidecar('steered', [prompt, steer])
const started = a.rpc.find((r) => r.method === 'turn/start')
const steered = a.rpc.find((r) => r.method === 'turn/steer')
const steeredOk = a.methods.join(',') === 'thread/start,turn/start,turn/steer' &&
  steered.params.threadId === 'fixture-thread' &&
  steered.params.expectedTurnId === `turn-${started.id}` &&
  JSON.stringify(steered.params.input) === JSON.stringify([
    { type: 'text', text: 'use tabs', text_elements: [] },
    { type: 'image', url: 'data:image/png;base64,AAAA' },
  ]) &&
  started.params.input[0].text === 'build it' &&
  a.warnings.length === 0 &&
  JSON.stringify(a.steeredAcks) === JSON.stringify([{ type: '_puppeteer_steered', text: steer.text, images: steer.images }])

const b = await runSidecar('no-turn', [steer])
const noTurnOk = b.methods.join(',') === 'thread/start' &&
  b.warnings.length === 1 && /Steer not applied: no turn is running/.test(b.warnings[0]) &&
  b.steeredAcks.length === 0

const c = await runSidecar('refused', [prompt, steer], { PUPPETEER_FIXTURE_STEER_ERROR: 'no active turn to steer' })
const refusedOk = c.methods.join(',') === 'thread/start,turn/start,turn/steer' &&
  c.warnings.length === 1 && /Steer not applied: no active turn to steer/.test(c.warnings[0]) &&
  c.steeredAcks.length === 0

const pass = !!(steeredOk && noTurnOk && refusedOk)
console.log(JSON.stringify({ steeredOk, noTurnOk, refusedOk, methods: [a.methods, b.methods, c.methods], warnings: [a.warnings, b.warnings, c.warnings], acks: [a.steeredAcks.length, b.steeredAcks.length, c.steeredAcks.length], pass }))
process.exit(pass ? 0 : 1)
