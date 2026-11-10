// Reasoning effort (UX-16) reaches Codex as `effort` on every turn/start —
// fresh and resumed threads alike — and is left out when unset or when the level
// is one Codex does not have (`max`).
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync, readFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-framing.mjs')

function runSidecar(name, env, prompts) {
  const work = mkdtempSync(join(tmpdir(), `puppeteer-codex-effort-${name}-`))
  const rpcOut = join(work, 'rpc.ndjson')
  const childEnv = { ...process.env }
  for (const k of ['PUPPETEER_RESUME_SESSION', 'PUPPETEER_CODEX_EFFORT', 'PUPPETEER_CODEX_MCP', 'PUPPETEER_FIXTURE_COMPLETE_MS']) delete childEnv[k]
  return new Promise((resolve) => {
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...childEnv, PUPPETEER_PERMISSION_MODE: 'default', PUPPETEER_CODEX_PATH: fixture, PUPPETEER_FRAMING_RPC_OUT: rpcOut, ...env },
    })
    child.stdout.on('data', () => {})
    for (const text of prompts) child.stdin.write(JSON.stringify({ type: 'prompt', text }) + '\n')
    setTimeout(() => {
      try { child.kill() } catch {}
      const rpc = existsSync(rpcOut)
        ? readFileSync(rpcOut, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l))
        : []
      resolve(rpc.filter((r) => r.method === 'turn/start').map((r) => r.params))
    }, 1500)
  })
}

// Two turns: the level rides on each of them.
const fresh = await runSidecar('fresh', { PUPPETEER_CODEX_EFFORT: 'high', PUPPETEER_FIXTURE_COMPLETE_MS: '200' }, ['one', 'two'])
const freshOk = fresh.length === 2 && fresh.every((p) => p.effort === 'high')

const resumed = await runSidecar('resume', { PUPPETEER_RESUME_SESSION: 'thr-old', PUPPETEER_CODEX_EFFORT: 'xhigh' }, ['again'])
const resumeOk = resumed.length === 1 && resumed[0].effort === 'xhigh'

const unset = await runSidecar('unset', {}, ['plain'])
const unsetOk = unset.length === 1 && !('effort' in unset[0])

const max = await runSidecar('max', { PUPPETEER_CODEX_EFFORT: 'max' }, ['plain'])
const maxOk = max.length === 1 && !('effort' in max[0])

const pass = !!(freshOk && resumeOk && unsetOk && maxOk)
console.log(JSON.stringify({ freshOk, resumeOk, unsetOk, maxOk, pass }))
process.exit(pass ? 0 : 1)
