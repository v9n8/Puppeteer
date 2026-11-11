// Fork on Codex (UX-28): a resume with PUPPETEER_FORK_SESSION=1 copies the
// thread (thread/fork, never thread/resume) and goes on in the copy, rolled
// back to the turns the fork keeps — before system.init, which names the new
// thread, and before any turn. A server that hands back the source thread
// stops the run: the source is never continued in place. Against a fixture
// like codex-cli 0.133 (thread/rollback) and one like openai/codex@d15bf1d
// (thread/revert only, PUPPETEER_FIXTURE_CODEX=head).
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync, readFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-framing.mjs')

function runSidecar(name, env) {
  const work = mkdtempSync(join(tmpdir(), `puppeteer-codex-fork-${name}-`))
  const rpcOut = join(work, 'rpc.ndjson')
  const childEnv = { ...process.env }
  for (const k of [
    'PUPPETEER_RESUME_SESSION', 'PUPPETEER_ROLLBACK_TURNS', 'PUPPETEER_FORK_SESSION', 'PUPPETEER_FORK_KEEP_TURNS',
    'PUPPETEER_FIXTURE_TURNS', 'PUPPETEER_FIXTURE_FORK_SAME_ID', 'PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS',
    'PUPPETEER_CODEX_SEED_FRAMING', 'PUPPETEER_CODEX_MCP', 'PUPPETEER_CODEX_VERSION', 'PUPPETEER_FIXTURE_CODEX',
    'PUPPETEER_FIXTURE_HISTORY_MODE',
  ]) delete childEnv[k]
  return new Promise((resolve) => {
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...childEnv, PUPPETEER_PERMISSION_MODE: 'default', PUPPETEER_CODEX_PATH: fixture, PUPPETEER_FRAMING_RPC_OUT: rpcOut, ...env },
    })
    let init = null
    let buf = ''
    child.stdout.on('data', (d) => {
      buf += d
      let nl
      while ((nl = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, nl); buf = buf.slice(nl + 1)
        try {
          const m = JSON.parse(line)
          if (m.type === 'system' && m.subtype === 'init' && !init) init = m
        } catch {}
      }
    })
    child.stdin.write(JSON.stringify({ type: 'prompt', text: 'again' }) + '\n')
    setTimeout(() => {
      try { child.kill() } catch {}
      const rpc = existsSync(rpcOut)
        ? readFileSync(rpcOut, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l))
        : []
      resolve({ methods: rpc.map((r) => r.method), rpc, init })
    }, 1500)
  })
}

// Keep 2 of the 5 turns the copy holds: roll the copy back 3, then go on in it.
const a = await runSidecar('keep', { PUPPETEER_RESUME_SESSION: 'thr-src', PUPPETEER_FORK_SESSION: '1', PUPPETEER_FORK_KEEP_TURNS: '2', PUPPETEER_FIXTURE_TURNS: '5' })
const rollback = a.rpc.find((r) => r.method === 'thread/rollback')
const turn = a.rpc.find((r) => r.method === 'turn/start')
const keepOk = a.methods.join(',') === 'thread/fork,thread/rollback,turn/start' &&
  a.rpc[0].params.threadId === 'thr-src' &&
  rollback.params.threadId === 'fork-of-thr-src' && rollback.params.numTurns === 3 &&
  turn.params.threadId === 'fork-of-thr-src' && a.init?.session_id === 'fork-of-thr-src'

// The whole thread kept: nothing to roll back.
const b = await runSidecar('whole', { PUPPETEER_RESUME_SESSION: 'thr-src', PUPPETEER_FORK_SESSION: '1', PUPPETEER_FORK_KEEP_TURNS: '5', PUPPETEER_FIXTURE_TURNS: '5' })
const wholeOk = b.methods.join(',') === 'thread/fork,turn/start' && b.init?.session_id === 'fork-of-thr-src'

// A server that does not copy the thread: no turn runs, no init.
const c = await runSidecar('same-id', { PUPPETEER_RESUME_SESSION: 'thr-src', PUPPETEER_FORK_SESSION: '1', PUPPETEER_FORK_KEEP_TURNS: '1', PUPPETEER_FIXTURE_TURNS: '3', PUPPETEER_FIXTURE_FORK_SAME_ID: '1' })
const refusedOk = c.methods.join(',') === 'thread/fork' && c.init === null

// Without a thread to resume there is nothing to fork; a plain resume does not fork.
const d = await runSidecar('no-resume', { PUPPETEER_FORK_SESSION: '1', PUPPETEER_FORK_KEEP_TURNS: '1' })
const freshOk = d.methods.join(',') === 'thread/start,turn/start'
const e = await runSidecar('plain', { PUPPETEER_RESUME_SESSION: 'thr-src', PUPPETEER_FORK_KEEP_TURNS: '1', PUPPETEER_FIXTURE_TURNS: '3' })
const plainOk = e.methods.join(',') === 'thread/resume,turn/start'

// HEAD: the copy is reverted before its first dropped turn (the 3rd), then goes on.
const h = await runSidecar('head', { PUPPETEER_FIXTURE_CODEX: 'head', PUPPETEER_RESUME_SESSION: 'thr-src', PUPPETEER_FORK_SESSION: '1', PUPPETEER_FORK_KEEP_TURNS: '2', PUPPETEER_FIXTURE_TURNS: '5' })
const revert = h.rpc.find((r) => r.method === 'thread/revert')
const headOk = h.methods.join(',') === 'thread/fork,thread/revert,turn/start' &&
  revert.params.threadId === 'fork-of-thr-src' && revert.params.beforeTurnId === 't2' &&
  h.rpc.find((r) => r.method === 'turn/start').params.threadId === 'fork-of-thr-src' && h.init?.session_id === 'fork-of-thr-src'
const hw = await runSidecar('head-whole', { PUPPETEER_FIXTURE_CODEX: 'head', PUPPETEER_RESUME_SESSION: 'thr-src', PUPPETEER_FORK_SESSION: '1', PUPPETEER_FORK_KEEP_TURNS: '5', PUPPETEER_FIXTURE_TURNS: '5' })
const headWholeOk = hw.methods.join(',') === 'thread/fork,turn/start'

const pass = !!(keepOk && wholeOk && refusedOk && freshOk && plainOk && headOk && headWholeOk)
console.log(JSON.stringify({ keepOk, wholeOk, refusedOk, freshOk, plainOk, headOk, headWholeOk, methods: [a.methods, b.methods, c.methods, d.methods, e.methods, h.methods], pass }))
process.exit(pass ? 0 : 1)
