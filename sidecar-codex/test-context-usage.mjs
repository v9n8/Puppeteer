// Codex context measurement (AC-08): each turn that made a model request is
// preceded by one `_puppeteer_context_usage` — the turn's LAST request (input
// incl. cached + output) against the model's window, never the thread's
// running `total`. A window codex did not report is null, a window seen
// earlier (the resume replay included) stays known, and a turn that made no
// request (or never started) reports nothing.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-app-server.mjs')

// Each prompt in `prompts` is sent once the session is ready; resolves on exit.
function runSidecar(name, script, { resume = false, prompts = 1 } = {}) {
  const work = mkdtempSync(join(tmpdir(), `puppeteer-codex-context-${name}-`))
  const childEnv = { ...process.env }
  for (const k of ['PUPPETEER_RESUME_SESSION', 'PUPPETEER_ROLLBACK_TURNS', 'PUPPETEER_CODEX_KEEP_OPEN', 'PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS', 'PUPPETEER_CODEX_SEED_FRAMING', 'PUPPETEER_CODEX_MCP', 'PUPPETEER_FIXTURE_LOG']) delete childEnv[k]
  return new Promise((resolve) => {
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: {
        ...childEnv,
        PUPPETEER_PERMISSION_MODE: 'default',
        PUPPETEER_CODEX_PATH: fixture,
        PUPPETEER_FIXTURE_USAGE: JSON.stringify(script),
        ...(resume ? { PUPPETEER_RESUME_SESSION: 'fixture-thread' } : {}),
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
        if (msg.type === '_puppeteer_ready') {
          for (let i = 0; i < prompts; i++) child.stdin.write(JSON.stringify({ type: 'prompt', text: `p${i}` }) + '\n')
        }
      }
    })
    // A hang fails the test instead of blocking CI.
    const timer = setTimeout(() => { try { child.kill('SIGKILL') } catch {} }, 8000)
    child.on('exit', (code) => {
      clearTimeout(timer)
      // The messages that end each turn, in order: a context event, then its result.
      const ends = out.filter((m) => m.type === 'result' || m.type === '_puppeteer_context_usage')
      resolve({ code, ends: ends.map(({ type, ...rest }) => (type === 'result' ? 'result' : rest)) })
    })
  })
}

const bd = (input, cached, output) => ({ totalTokens: input + output, inputTokens: input, cachedInputTokens: cached, outputTokens: output, reasoningOutputTokens: 0 })
const ctx = (totalTokens, maxTokens) => ({ model: 'fixture', totalTokens, maxTokens })
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b)

// 1. Three requests in one turn: the context is the last one (1700 + 80), not
//    the turn's sum nor the thread's total.
const multi = await runSidecar('multi', { turns: [{ updates: [
  { total: bd(1000, 800, 50), last: bd(1000, 800, 50), modelContextWindow: 258400 },
  { total: bd(2700, 2300, 130), last: bd(1700, 1500, 80), modelContextWindow: 258400 },
] }] })
const multiOk = multi.code === 0 && same(multi.ends, [ctx(1780, 258400), 'result'])

// 2. Four prompts: a turn; a refused turn/start; a turn with no request; a
//    turn whose updates carry no window (the one seen before stays).
const seq = await runSidecar('sequence', { turns: [
  { updates: [{ total: bd(1000, 0, 100), last: bd(1000, 0, 100), modelContextWindow: 272000 }] },
  { fail: true },
  { updates: [] },
  { updates: [{ total: bd(3500, 900, 150), last: bd(2500, 900, 50) }] },
] }, { prompts: 4 })
const sequenceOk = seq.code === 0 && same(seq.ends, [ctx(1100, 272000), 'result', 'result', ctx(2550, 272000), 'result'])

// 3. A resumed thread: the replay names no window (null) and the turn's
//    update none either → maxTokens null; the replayed total is not a context.
const resumed = await runSidecar('resumed', { replay: bd(90000, 80000, 4000), turns: [
  { updates: [{ total: bd(91000, 80800, 4050), last: bd(1000, 800, 50) }] },
] }, { resume: true })
const resumedOk = resumed.code === 0 && same(resumed.ends, [ctx(1050, null), 'result'])

const pass = !!(multiOk && sequenceOk && resumedOk)
console.log(JSON.stringify({ multiOk, sequenceOk, resumedOk, ends: [multi.ends, seq.ends, resumed.ends], pass }))
process.exit(pass ? 0 : 1)
