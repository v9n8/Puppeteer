// Rewind on Codex: a resume with PUPPETEER_ROLLBACK_TURNS rolls the thread back
// right after thread/resume — before system.init (which tells the broker the cut
// is applied) and before any turn, and never more turns than the thread holds.
// Without a resume, or with no turns to drop, nothing is rolled back.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync, readFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-framing.mjs')

function runSidecar(name, env) {
  const work = mkdtempSync(join(tmpdir(), `puppeteer-codex-rewind-${name}-`))
  const rpcOut = join(work, 'rpc.ndjson')
  const childEnv = { ...process.env }
  for (const k of ['PUPPETEER_RESUME_SESSION', 'PUPPETEER_ROLLBACK_TURNS', 'PUPPETEER_FIXTURE_TURNS', 'PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS', 'PUPPETEER_CODEX_SEED_FRAMING', 'PUPPETEER_CODEX_MCP']) delete childEnv[k]
  return new Promise((resolve) => {
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...childEnv, PUPPETEER_PERMISSION_MODE: 'default', PUPPETEER_CODEX_PATH: fixture, PUPPETEER_FRAMING_RPC_OUT: rpcOut, ...env },
    })
    // RPC lines recorded when system.init arrives: the fixture records a call
    // before answering it, so >= 2 means init came after the rollback.
    let initAfterRpc = -1
    let buf = ''
    child.stdout.on('data', (d) => {
      buf += d
      let nl
      while ((nl = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, nl); buf = buf.slice(nl + 1)
        try {
          const m = JSON.parse(line)
          if (m.type === 'system' && m.subtype === 'init' && initAfterRpc < 0) {
            initAfterRpc = existsSync(rpcOut) ? readFileSync(rpcOut, 'utf8').trim().split('\n').length : 0
          }
        } catch {}
      }
    })
    child.stdin.write(JSON.stringify({ type: 'prompt', text: 'again' }) + '\n')
    setTimeout(() => {
      try { child.kill() } catch {}
      const rpc = existsSync(rpcOut)
        ? readFileSync(rpcOut, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l))
        : []
      resolve({ methods: rpc.map((r) => r.method), rpc, initAfterRpc })
    }, 1500)
  })
}

const a = await runSidecar('rollback', { PUPPETEER_RESUME_SESSION: 'thr-old', PUPPETEER_ROLLBACK_TURNS: '2' })
const rollback = a.rpc.find((r) => r.method === 'thread/rollback')
const rollbackOk = a.methods.join(',') === 'thread/resume,thread/rollback,turn/start' &&
  rollback.params.threadId === 'thr-old' && rollback.params.numTurns === 2 &&
  a.initAfterRpc >= 2

// Never more turns than the thread holds.
const clamped = await runSidecar('clamp', { PUPPETEER_RESUME_SESSION: 'thr-old', PUPPETEER_ROLLBACK_TURNS: '5', PUPPETEER_FIXTURE_TURNS: '3' })
const clampOk = clamped.rpc.find((r) => r.method === 'thread/rollback')?.params.numTurns === 3

const b = await runSidecar('plain-resume', { PUPPETEER_RESUME_SESSION: 'thr-old' })
const plainOk = b.methods.join(',') === 'thread/resume,turn/start'

const c = await runSidecar('no-resume', { PUPPETEER_ROLLBACK_TURNS: '2' })
const freshOk = c.methods.join(',') === 'thread/start,turn/start'

const pass = !!(rollbackOk && clampOk && plainOk && freshOk)
console.log(JSON.stringify({ rollbackOk, clampOk, plainOk, freshOk, methods: [a.methods, b.methods, c.methods], pass }))
process.exit(pass ? 0 : 1)
