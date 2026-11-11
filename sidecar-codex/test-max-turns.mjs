// Model-call limit of an unattended turn (AC-23): codex has no option for it,
// so the sidecar counts the turn's usage updates (one per model request) and
// interrupts the turn once PUPPETEER_MAX_TURNS of them came. The turn then
// ends with `_puppeteer_limit_reached` and an `error_max_turns` result, like
// Claude's. Without the env nothing is counted; a turn that ends on its own
// with its last allowed request is no limit stop.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-app-server.mjs')

function runSidecar(name, script, env = {}) {
  const work = mkdtempSync(join(tmpdir(), `puppeteer-codex-max-turns-${name}-`))
  const log = join(work, 'fixture.log')
  const childEnv = { ...process.env }
  for (const k of ['PUPPETEER_RESUME_SESSION', 'PUPPETEER_ROLLBACK_TURNS', 'PUPPETEER_CODEX_KEEP_OPEN', 'PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS', 'PUPPETEER_CODEX_SEED_FRAMING', 'PUPPETEER_CODEX_MCP', 'PUPPETEER_MAX_TURNS']) delete childEnv[k]
  return new Promise((resolve) => {
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: {
        ...childEnv,
        PUPPETEER_PERMISSION_MODE: 'default',
        PUPPETEER_CODEX_PATH: fixture,
        PUPPETEER_FIXTURE_USAGE: JSON.stringify(script),
        PUPPETEER_FIXTURE_LOG: log,
        ...env,
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
        if (msg.type === '_puppeteer_ready') child.stdin.write(JSON.stringify({ type: 'prompt', text: 'go' }) + '\n')
      }
    })
    // A hang fails the test instead of blocking CI.
    const timer = setTimeout(() => { try { child.kill('SIGKILL') } catch {} }, 8000)
    child.on('exit', (code) => {
      clearTimeout(timer)
      const requests = readFileSync(log, 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l))
      resolve({
        code,
        out,
        results: out.filter((m) => m.type === 'result'),
        limits: out.filter((m) => m.type === '_puppeteer_limit_reached'),
        errors: out.filter((m) => m.type === '_puppeteer_error'),
        warns: out.filter((m) => m.type === '_puppeteer_log' && m.level === 'warn').map((m) => m.text),
        interrupts: requests.filter((r) => r.method === 'turn/interrupt'),
      })
    })
  })
}

// One model request's update: `last` is that request, `total` the thread's sum.
const update = (n) => ({
  total: { inputTokens: 100 * n, cachedInputTokens: 0, outputTokens: 10 * n },
  last: { inputTokens: 100, cachedInputTokens: 0, outputTokens: 10 },
})
const paced = { turns: [{ everyMs: 15, maxUpdates: 12, updates: Array.from({ length: 12 }, (_, i) => update(i + 1)) }] }

// 1. Limit 3: interrupted after its third request, an error_max_turns result
//    right after the limit line, and no error that would read as a crash.
const limited = await runSidecar('limited', paced, { PUPPETEER_MAX_TURNS: '3' })
const [limitedResult] = limited.results
const at = (m) => limited.out.indexOf(m)
const limitedOk = limited.code === 0 &&
  limited.interrupts.length === 1 && limited.interrupts[0].params?.turnId === 'turn-0' &&
  limited.limits.length === 1 && limited.limits[0].kind === 'turns' && limited.limits[0].limit === 3 &&
  limited.results.length === 1 && limitedResult.subtype === 'error_max_turns' && limitedResult.is_error === true &&
  limitedResult.num_turns >= 3 && limitedResult.num_turns < 12 &&
  at(limited.limits[0]) < at(limitedResult) &&
  limited.errors.length === 0 && limited.warns.length === 0

// 2. No env: the turn makes all its requests and succeeds; nothing interrupts it.
const free = await runSidecar('free', paced)
const freeOk = free.code === 0 && free.interrupts.length === 0 && free.limits.length === 0 &&
  free.results.length === 1 && free.results[0].subtype === 'success'

// 3. The last allowed request is the turn's last one: the turn completes on its
//    own (an interrupt sent meanwhile changes nothing), a success, no limit stop.
const exact = await runSidecar('exact', { turns: [{ updates: [update(1), update(2)] }] }, { PUPPETEER_MAX_TURNS: '2' })
const exactOk = exact.code === 0 && exact.limits.length === 0 &&
  exact.results.length === 1 && exact.results[0].subtype === 'success'

// 4. Not a whole number above 0: no limit.
const zero = await runSidecar('zero', paced, { PUPPETEER_MAX_TURNS: '0' })
const zeroOk = zero.interrupts.length === 0 && zero.limits.length === 0 && zero.results[0]?.subtype === 'success'

const pass = !!(limitedOk && freeOk && exactOk && zeroOk)
console.log(JSON.stringify({
  limitedOk, freeOk, exactOk, zeroOk,
  results: [limited, free, exact, zero].map((r) => r.results.map((x) => `${x.subtype}/${x.num_turns}`)),
  interrupts: [limited, free, exact, zero].map((r) => r.interrupts.length),
  warns: [limited, free, exact, zero].map((r) => r.warns),
  pass,
}))
process.exit(pass ? 0 : 1)
