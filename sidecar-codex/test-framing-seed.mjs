// Framing reaches Codex the right way:
//  (A) fresh start → `developerInstructions` on thread/start, reported seeded;
//  (B) resume → `developerInstructions` and `model` on thread/resume too (codex
//      applies them from the next compact), and the <session-framing> seed is
//      prepended to the FIRST turn only, reported seeded once;
//  (D) fork → the same on thread/fork; no seed, nothing reported;
//  (E) a codex that refuses the params → resumed with the bare thread id, the
//      seed still prepended.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync, readFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-framing.mjs')

function runSidecar(name, env, prompts) {
  const work = mkdtempSync(join(tmpdir(), `puppeteer-codex-framing-${name}-`))
  const rpcOut = join(work, 'rpc.ndjson')
  const childEnv = { ...process.env }
  for (const k of [
    'PUPPETEER_RESUME_SESSION', 'PUPPETEER_FORK_SESSION', 'PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS', 'PUPPETEER_CODEX_SEED_FRAMING',
    'PUPPETEER_CODEX_STYLE_INSTRUCTIONS', 'PUPPETEER_CODEX_SEED_STYLE', 'PUPPETEER_CODEX_MODEL', 'PUPPETEER_CODEX_MCP',
    'PUPPETEER_FIXTURE_COMPLETE_MS', 'PUPPETEER_FIXTURE_REFUSE_POLICY',
  ]) delete childEnv[k]
  return new Promise((resolve) => {
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...childEnv, PUPPETEER_PERMISSION_MODE: 'default', PUPPETEER_CODEX_PATH: fixture, PUPPETEER_FRAMING_RPC_OUT: rpcOut, ...env },
    })
    const ctrl = []
    let buf = ''
    child.stdout.on('data', (d) => {
      buf += d
      let nl
      while ((nl = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, nl); buf = buf.slice(nl + 1)
        try { const m = JSON.parse(line); if (m.type?.startsWith('_puppeteer_')) ctrl.push(m.type) } catch {}
      }
    })
    for (const text of prompts) child.stdin.write(JSON.stringify({ type: 'prompt', text }) + '\n')
    setTimeout(() => {
      try { child.kill() } catch {}
      const rpc = existsSync(rpcOut)
        ? readFileSync(rpcOut, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l))
        : []
      resolve({ rpc, ctrl })
    }, 1500)
  })
}

const seeded = (ctrl) => ctrl.filter((t) => t === '_puppeteer_framing_seeded').length
const turnTexts = (rpc) => rpc.filter((r) => r.method === 'turn/start').map((r) => r.params.input?.[0]?.text)

const a = await runSidecar('start', { PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS: 'FRAMING-A' }, ['hello'])
const aStart = a.rpc.find((r) => r.method === 'thread/start')
const startOk = aStart?.params?.developerInstructions === 'FRAMING-A' &&
  seeded(a.ctrl) === 1 && turnTexts(a.rpc)[0] === 'hello'

const seed = '<session-framing>\nFRAMING-B\n</session-framing>'
// Turns complete here: the second prompt waits for the first (D4-15).
const b = await runSidecar('resume', {
  PUPPETEER_RESUME_SESSION: 'thr-old',
  PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS: 'FRAMING-B',
  PUPPETEER_CODEX_MODEL: 'gpt-b',
  PUPPETEER_CODEX_SEED_FRAMING: seed,
  PUPPETEER_FIXTURE_COMPLETE_MS: '200',
}, ['first', 'second'])
const bResume = b.rpc.find((r) => r.method === 'thread/resume')
const texts = turnTexts(b.rpc)
const resumeOk = bResume?.params?.developerInstructions === 'FRAMING-B' && bResume.params.model === 'gpt-b' &&
  texts[0] === `${seed}\n\nfirst` && texts[1] === 'second' && seeded(b.ctrl) === 1

const c = await runSidecar('plain', {}, ['plain'])
const cStart = c.rpc.find((r) => r.method === 'thread/start')?.params || {}
// A regular thread is kept on disk so it can be resumed (only side tasks are ephemeral).
const plainOk = seeded(c.ctrl) === 0 && turnTexts(c.rpc)[0] === 'plain' &&
  !('developerInstructions' in cStart) && !('ephemeral' in cStart)

// A thread already framed gets no seed; its fork still carries the framing.
const d = await runSidecar('fork', {
  PUPPETEER_RESUME_SESSION: 'thr-src',
  PUPPETEER_FORK_SESSION: '1',
  PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS: 'FRAMING-D',
  PUPPETEER_CODEX_MODEL: 'gpt-d',
}, ['go'])
const dFork = d.rpc.find((r) => r.method === 'thread/fork')
const forkOk = dFork?.params?.developerInstructions === 'FRAMING-D' && dFork.params.model === 'gpt-d' &&
  !d.rpc.some((r) => r.method === 'thread/resume') && turnTexts(d.rpc)[0] === 'go' && seeded(d.ctrl) === 0

const e = await runSidecar('refused', {
  PUPPETEER_RESUME_SESSION: 'thr-old',
  PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS: 'FRAMING-E',
  PUPPETEER_CODEX_MODEL: 'gpt-e',
  PUPPETEER_CODEX_SEED_FRAMING: seed,
  PUPPETEER_FIXTURE_REFUSE_POLICY: '1',
}, ['first'])
const eResumes = e.rpc.filter((r) => r.method === 'thread/resume').map((r) => r.params)
const refusedOk = eResumes.length === 2 && eResumes[0].developerInstructions === 'FRAMING-E' &&
  JSON.stringify(eResumes[1]) === JSON.stringify({ threadId: 'thr-old' }) &&
  turnTexts(e.rpc)[0] === `${seed}\n\nfirst` && seeded(e.ctrl) === 1

const pass = !!(startOk && resumeOk && plainOk && forkOk && refusedOk)
console.log(JSON.stringify({ startOk, resumeOk, plainOk, forkOk, refusedOk, texts, pass }))
process.exit(pass ? 0 : 1)
