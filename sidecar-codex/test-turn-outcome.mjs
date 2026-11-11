// Codex turn outcome (AC-03): a `failed` turn ends with an error result and
// one readable `_puppeteer_error` (so the broker ends the run `failed`); an
// `interrupted` turn is no error; an `error` codex will retry is a warning only;
// a later successful turn still clears an earlier failure.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-app-server.mjs')

// Each prompt in `prompts` is sent once the session is ready; resolves on exit.
function runSidecar(name, script, { prompts = 1 } = {}) {
  const work = mkdtempSync(join(tmpdir(), `puppeteer-codex-outcome-${name}-`))
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
      resolve({
        code,
        out,
        results: out.filter((m) => m.type === 'result'),
        errors: out.filter((m) => m.type === '_puppeteer_error').map((m) => m.error),
        warns: out.filter((m) => m.type === '_puppeteer_log' && m.level === 'warn').map((m) => m.text),
      })
    })
  })
}

// The run's final status as the broker decides it (src-tauri/src/runs/sidecar.rs,
// `unresolved_error` + `final_run_status`): a `_puppeteer_error` sets the flag,
// a `success` result clears it.
function brokerStatus(run) {
  let unresolved = false
  for (const m of run.out) {
    if (m.type === '_puppeteer_error') unresolved = true
    else if (m.type === 'result' && m.subtype === 'success') unresolved = false
  }
  return unresolved || run.code !== 0 ? 'failed' : 'done'
}

const usageLimit = { message: "You've hit your usage limit.", codexErrorInfo: 'usageLimitExceeded', additionalDetails: null }

// 1. Out of quota: codex sends the final `error` (willRetry: false), then
//    turn/completed `failed` with the same error. One error line, not two.
const quota = await runSidecar('quota', { turns: [{
  errors: [{ error: usageLimit, willRetry: false }],
  status: 'failed',
  error: usageLimit,
}] })
const quotaResult = quota.results[0]
const quotaOk = quota.results.length === 1 &&
  quotaResult.subtype === 'error_during_execution' && quotaResult.is_error === true &&
  quotaResult.codex_error_info === 'usageLimitExceeded' &&
  /usage limit reached/i.test(quotaResult.errors?.[0] || '') &&
  quota.errors.length === 1 && /usage limit reached: You've hit your usage limit/i.test(quota.errors[0]) &&
  brokerStatus(quota) === 'failed'

// 2. A failed turn with no `error` notification before it, and a code that
//    carries an HTTP status: the reason is reported at turn/completed.
const httpError = { message: 'Connection failed', codexErrorInfo: { httpConnectionFailed: { httpStatusCode: 502 } }, additionalDetails: 'upstream 502' }
const http = await runSidecar('http', { turns: [{ status: 'failed', error: httpError }] })
const httpOk = http.results.length === 1 && http.results[0].is_error === true &&
  http.results[0].codex_error_info?.httpConnectionFailed?.httpStatusCode === 502 &&
  http.errors.length === 1 && http.errors[0] === 'Connection failed (upstream 502)' &&
  brokerStatus(http) === 'failed'

// 3. Interrupted: a result that is no error, and the run is not failed.
const stop = await runSidecar('interrupted', { turns: [{ status: 'interrupted' }] })
const stopOk = stop.results.length === 1 && stop.results[0].subtype === 'interrupted' &&
  stop.results[0].is_error === false && stop.errors.length === 0 && brokerStatus(stop) === 'done'

// 4. Reconnecting: `error` with willRetry: true is a warning, the turn goes on
//    and succeeds.
const disconnected = (n) => ({
  error: { message: `Reconnecting... ${n}/5`, codexErrorInfo: { responseStreamDisconnected: { httpStatusCode: null } }, additionalDetails: 'stream disconnected before completion' },
  willRetry: true,
})
const retry = await runSidecar('retry', { turns: [{ errors: [disconnected(1), disconnected(2)] }] })
const retryOk = retry.results.length === 1 && retry.results[0].subtype === 'success' && !('is_error' in retry.results[0]) &&
  retry.errors.length === 0 &&
  retry.warns.length === 2 && retry.warns[1] === '⚠ Codex: Reconnecting... 2/5 (stream disconnected before completion)' &&
  brokerStatus(retry) === 'done'

// 5. One session, a failed turn then a successful one: the success clears the
//    failure, as it does for every engine.
const recover = await runSidecar('recover', { turns: [
  { errors: [{ error: usageLimit, willRetry: false }], status: 'failed', error: usageLimit },
  {},
] }, { prompts: 2 })
const recoverOk = recover.results.length === 2 && recover.results[0].is_error === true &&
  recover.results[1].subtype === 'success' && recover.errors.length === 1 && brokerStatus(recover) === 'done'

const pass = !!(quotaOk && httpOk && stopOk && retryOk && recoverOk)
console.log(JSON.stringify({
  quotaOk, httpOk, stopOk, retryOk, recoverOk,
  results: [quota, http, stop, retry, recover].map((r) => r.results.map((x) => x.subtype)),
  errors: [quota, http, stop, retry, recover].map((r) => r.errors),
  pass,
}))
process.exit(pass ? 0 : 1)
