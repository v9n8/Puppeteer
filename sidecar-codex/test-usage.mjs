// Codex token usage (AC-02): a turn's `result.usage` is how much the thread's
// `total` grew over the turn, never the `last` request alone; reasoning tokens
// are not added on top of output (they are part of it); the total codex
// replays on resume only moves the baseline, even when it lands after
// turn/start went out; a refused turn/start, or a turn that made no request,
// carries no usage (none left over from the turn before).
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-app-server.mjs')

// Each prompt in `prompts` is sent once the session is ready; resolves on exit.
function runSidecar(name, script, { resume = false, prompts = 1 } = {}) {
  const work = mkdtempSync(join(tmpdir(), `puppeteer-codex-usage-${name}-`))
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
      resolve({ code, results: out.filter((m) => m.type === 'result'), failed: out.filter((m) => m.type === '_puppeteer_turn_failed').length })
    })
  })
}

// A codex TokenUsageBreakdown (inputTokens includes the cached part).
const bd = (input, cached, output, reasoning = 0) => ({
  totalTokens: input + output,
  inputTokens: input,
  cachedInputTokens: cached,
  outputTokens: output,
  reasoningOutputTokens: reasoning,
})
const update = (total, last) => ({ total, last, modelContextWindow: 258400 })
// The Claude-shaped usage the sidecar should report.
const usage = (input, cached, output) => ({ input_tokens: input, output_tokens: output, cache_creation_input_tokens: 0, cache_read_input_tokens: cached })
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b)

// 1. One turn, three model requests (tool calls between them), on a new thread.
//    `last` alone would give 200 input / 1500 cached / 80 output.
const multi = await runSidecar('multi', { turns: [{ updates: [
  update(bd(1000, 800, 50, 30), bd(1000, 800, 50, 30)),
  update(bd(2500, 2000, 120, 60), bd(1500, 1200, 70, 30)),
  update(bd(4200, 3500, 200, 90), bd(1700, 1500, 80, 30)),
] }] })
const multiOk = multi.code === 0 && multi.results.length === 1 &&
  same(multi.results[0].usage, usage(700, 3500, 200))

// 2–4. A resumed thread whose stored total is 50000 / 40000 / 3000: the first
//      turn counts only its own two requests, whether codex replays that total
//      right after thread/resume, after turn/start went out, or not at all.
const replay = bd(50000, 40000, 3000, 1000)
const resumedTurn = { updates: [
  update(bd(51000, 40800, 3050, 1010), bd(1000, 800, 50, 10)),
  update(bd(52500, 42000, 3120, 1030), bd(1500, 1200, 70, 20)),
] }
const resumedUsage = usage(500, 2000, 120)
const replayed = await runSidecar('replay', { replay, turns: [resumedTurn] }, { resume: true })
const replayOk = replayed.code === 0 && same(replayed.results[0]?.usage, resumedUsage)
const late = await runSidecar('replay-late', { replay, replayLate: true, turns: [resumedTurn] }, { resume: true })
const replayLateOk = late.code === 0 && same(late.results[0]?.usage, resumedUsage)
const noReplay = await runSidecar('no-replay', { turns: [resumedTurn] }, { resume: true })
const noReplayOk = noReplay.code === 0 && same(noReplay.results[0]?.usage, resumedUsage)

// 5. Four prompts in one session: a turn with usage; a refused turn/start (no
//    result); a turn that made no request (no usage, nothing carried over);
//    then a turn counted from where the first one ended.
const seq = await runSidecar('sequence', { turns: [
  { updates: [update(bd(1000, 0, 100), bd(1000, 0, 100)), update(bd(3000, 1500, 250), bd(2000, 1500, 150))] },
  { fail: true },
  { updates: [] },
  { updates: [update(bd(5000, 3000, 300), bd(2000, 1500, 50))] },
] }, { prompts: 4 })
const sequenceOk = seq.code === 0 && seq.failed === 1 && seq.results.length === 3 &&
  same(seq.results[0].usage, usage(1500, 1500, 250)) &&
  !('usage' in seq.results[1]) &&
  same(seq.results[2].usage, usage(500, 1500, 50))

// 6. A codex that sends only `last` and names no turn: the turn's `last`s add up.
const lastOnly = await runSidecar('last-only', { turns: [{ noTurnId: true, updates: [
  { last: bd(1000, 600, 40, 20) },
  { last: bd(1200, 900, 60, 20) },
] }] })
const lastOnlyOk = lastOnly.code === 0 && same(lastOnly.results[0]?.usage, usage(700, 1500, 100))

const pass = !!(multiOk && replayOk && replayLateOk && noReplayOk && sequenceOk && lastOnlyOk)
console.log(JSON.stringify({
  multiOk, replayOk, replayLateOk, noReplayOk, sequenceOk, lastOnlyOk,
  usage: [multi, replayed, late, noReplay, seq, lastOnly].map((r) => r.results.map((x) => x.usage ?? null)),
  pass,
}))
process.exit(pass ? 0 : 1)
