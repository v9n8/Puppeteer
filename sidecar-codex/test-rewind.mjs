// Rewind on Codex: a resume with PUPPETEER_ROLLBACK_TURNS rolls the thread back
// right after thread/resume — before system.init (which tells the broker the cut
// is applied) and before any turn, and never more turns than the thread holds.
// Without a resume, or with no turns to drop, nothing is rolled back.
// Against a fixture like codex-cli 0.133 (thread/rollback) and one like
// openai/codex@d15bf1d (thread/revert only, PUPPETEER_FIXTURE_CODEX=head).
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
  for (const k of [
    'PUPPETEER_RESUME_SESSION', 'PUPPETEER_ROLLBACK_TURNS', 'PUPPETEER_FIXTURE_TURNS', 'PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS',
    'PUPPETEER_CODEX_SEED_FRAMING', 'PUPPETEER_CODEX_MCP', 'PUPPETEER_CODEX_VERSION', 'PUPPETEER_FIXTURE_CODEX', 'PUPPETEER_FIXTURE_HISTORY_MODE',
  ]) delete childEnv[k]
  return new Promise((resolve) => {
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...childEnv, PUPPETEER_PERMISSION_MODE: 'default', PUPPETEER_CODEX_PATH: fixture, PUPPETEER_FRAMING_RPC_OUT: rpcOut, ...env },
    })
    // RPC lines recorded when system.init arrives: the fixture records a call
    // before answering it, so >= 2 means init came after the rollback (or revert).
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

// A codex known to be 0.133 rolls back, and never tries thread/revert (it has
// none), not even when thread/rollback is refused: the run stops, no turn.
const old = await runSidecar('0133', { PUPPETEER_CODEX_VERSION: '0.133.0-alpha.1', PUPPETEER_RESUME_SESSION: 'thr-old', PUPPETEER_ROLLBACK_TURNS: '2', PUPPETEER_FIXTURE_TURNS: '5' })
const oldOk = old.methods.join(',') === 'thread/resume,thread/rollback,turn/start'
const oldRefused = await runSidecar('0133-refused', { PUPPETEER_CODEX_VERSION: '0.133.0-alpha.1', PUPPETEER_FIXTURE_CODEX: 'head', PUPPETEER_RESUME_SESSION: 'thr-old', PUPPETEER_ROLLBACK_TURNS: '2', PUPPETEER_FIXTURE_TURNS: '5' })
const oldRefusedOk = oldRefused.methods.join(',') === 'thread/resume,thread/rollback'

// HEAD, paginated thread: reverted before the first turn dropped, no rollback.
const head = await runSidecar('head', { PUPPETEER_FIXTURE_CODEX: 'head', PUPPETEER_RESUME_SESSION: 'thr-old', PUPPETEER_ROLLBACK_TURNS: '2', PUPPETEER_FIXTURE_TURNS: '5' })
const revert = head.rpc.find((r) => r.method === 'thread/revert')
const headOk = head.methods.join(',') === 'thread/resume,thread/revert,turn/start' &&
  revert.params.threadId === 'thr-old' && revert.params.beforeTurnId === 't3' && !('numTurns' in revert.params) &&
  head.initAfterRpc >= 2
// Never more turns than the thread holds: all 3 go.
const headClamped = await runSidecar('head-clamp', { PUPPETEER_FIXTURE_CODEX: 'head', PUPPETEER_RESUME_SESSION: 'thr-old', PUPPETEER_ROLLBACK_TURNS: '5', PUPPETEER_FIXTURE_TURNS: '3' })
const headClampOk = headClamped.rpc.find((r) => r.method === 'thread/revert')?.params.beforeTurnId === 't0'
// A thread that does not say it is paginated tries thread/rollback first; refused
// as unknown, it falls back to thread/revert before system.init.
const fallback = await runSidecar('head-fallback', { PUPPETEER_FIXTURE_CODEX: 'head', PUPPETEER_FIXTURE_HISTORY_MODE: 'none', PUPPETEER_RESUME_SESSION: 'thr-old', PUPPETEER_ROLLBACK_TURNS: '1', PUPPETEER_FIXTURE_TURNS: '4' })
const fallbackOk = fallback.methods.join(',') === 'thread/resume,thread/rollback,thread/revert,turn/start' &&
  fallback.rpc.find((r) => r.method === 'thread/revert').params.beforeTurnId === 't3' && fallback.initAfterRpc >= 3

const pass = !!(rollbackOk && clampOk && plainOk && freshOk && oldOk && oldRefusedOk && headOk && headClampOk && fallbackOk)
console.log(JSON.stringify({
  rollbackOk, clampOk, plainOk, freshOk, oldOk, oldRefusedOk, headOk, headClampOk, fallbackOk,
  methods: [a.methods, b.methods, c.methods, oldRefused.methods, head.methods, fallback.methods], pass,
}))
process.exit(pass ? 0 : 1)
