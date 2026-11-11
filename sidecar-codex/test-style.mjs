// Response style (UX-17) reaches Codex the right way:
//  (A) new thread → after the framing in `developerInstructions`; only the
//      framing is reported seeded, so a style alone never marks it;
//  (B) resumed thread whose style changed → the style goes in thread/resume's
//      `developerInstructions` too, and the broker's block is prepended to the
//      FIRST turn only (after a framing seed), with no seeded report of its own.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync, readFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-framing.mjs')
const SCRUB = [
  'PUPPETEER_RESUME_SESSION', 'PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS', 'PUPPETEER_CODEX_SEED_FRAMING',
  'PUPPETEER_CODEX_STYLE_INSTRUCTIONS', 'PUPPETEER_CODEX_SEED_STYLE', 'PUPPETEER_CODEX_MCP', 'PUPPETEER_FIXTURE_COMPLETE_MS',
]

function runSidecar(name, env, prompts) {
  const work = mkdtempSync(join(tmpdir(), `puppeteer-codex-style-${name}-`))
  const rpcOut = join(work, 'rpc.ndjson')
  const childEnv = { ...process.env }
  for (const k of SCRUB) delete childEnv[k]
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
const threadStart = (rpc) => rpc.find((r) => r.method === 'thread/start')?.params || {}

const a = await runSidecar('both', { PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS: 'FRAMING', PUPPETEER_CODEX_STYLE_INSTRUCTIONS: 'STYLE' }, ['hi'])
const bothOk = threadStart(a.rpc).developerInstructions === 'FRAMING\n\nSTYLE' && seeded(a.ctrl) === 1 && turnTexts(a.rpc)[0] === 'hi'

const b = await runSidecar('style-only', { PUPPETEER_CODEX_STYLE_INSTRUCTIONS: 'STYLE' }, ['hi'])
const styleOnlyOk = threadStart(b.rpc).developerInstructions === 'STYLE' && seeded(b.ctrl) === 0

const frame = '<session-framing>\nF\n</session-framing>'
const style = '<session-framing>\nS\n</session-framing>'
const c = await runSidecar('resume', {
  PUPPETEER_RESUME_SESSION: 'thr-old',
  PUPPETEER_CODEX_SEED_FRAMING: frame,
  PUPPETEER_CODEX_SEED_STYLE: style,
  PUPPETEER_CODEX_STYLE_INSTRUCTIONS: 'STYLE-NOW',
  PUPPETEER_FIXTURE_COMPLETE_MS: '200',
}, ['first', 'second'])
const texts = turnTexts(c.rpc)
const resumeOk = c.rpc.find((r) => r.method === 'thread/resume')?.params?.developerInstructions === 'STYLE-NOW' &&
  texts[0] === `${frame}\n\n${style}\n\nfirst` && texts[1] === 'second' && seeded(c.ctrl) === 1

const d = await runSidecar('resume-style-only', { PUPPETEER_RESUME_SESSION: 'thr-old', PUPPETEER_CODEX_SEED_STYLE: style }, ['again'])
const styleSeedOk = turnTexts(d.rpc)[0] === `${style}\n\nagain` && seeded(d.ctrl) === 0

const e = await runSidecar('plain', {}, ['plain'])
const plainOk = !('developerInstructions' in threadStart(e.rpc)) && turnTexts(e.rpc)[0] === 'plain'

const pass = !!(bothOk && styleOnlyOk && resumeOk && styleSeedOk && plainOk)
console.log(JSON.stringify({ bothOk, styleOnlyOk, resumeOk, styleSeedOk, plainOk, texts, pass }))
process.exit(pass ? 0 : 1)
