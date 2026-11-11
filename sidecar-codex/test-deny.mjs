// Regression test: a denied Codex command must never execute, and the deny
// reaches codex as the right decision — `decline` (the turn goes on) when the
// desktop answered, `cancel` (the turn stops) when a remote surface did or the
// answer names no origin.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { existsSync, mkdtempSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-app-server.mjs')

function runCase(origin) {
  return new Promise((resolve) => {
    const work = mkdtempSync(join(tmpdir(), 'puppeteer-codex-deny-'))
    const marker = join(work, 'DENIED_COMMAND_RAN')
    const decisionFile = join(work, 'decision')
    // Strip ambient Puppeteer runtime vars so the test is deterministic no matter who
    // runs it (e.g. inside a live Puppeteer run, PUPPETEER_RESUME_SESSION would make the
    // sidecar attempt `thread/resume`, which the fixture does not serve).
    const childEnv = { ...process.env }
    delete childEnv.PUPPETEER_RESUME_SESSION
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: {
        ...childEnv,
        PUPPETEER_PERMISSION_MODE: 'default',
        PUPPETEER_CODEX_PATH: fixture,
        PUPPETEER_DENY_TEST_MARKER: marker,
        PUPPETEER_DECISION_OUT: decisionFile,
      },
    })

    let buffer = ''
    let sawPermission = false
    let deniedResult = false
    let finished = false

    function finish() {
      if (finished) return
      finished = true
      try { child.kill() } catch {}
      const commandRan = existsSync(marker)
      const decision = existsSync(decisionFile) ? readFileSync(decisionFile, 'utf8') : null
      resolve({ origin: origin ?? null, sawPermission, deniedResult, commandRan, decision })
    }

    child.stdout.on('data', (chunk) => {
      buffer += chunk.toString()
      let newline
      while ((newline = buffer.indexOf('\n')) >= 0) {
        const line = buffer.slice(0, newline).trim()
        buffer = buffer.slice(newline + 1)
        if (!line) continue
        let message
        try { message = JSON.parse(line) } catch { continue }
        if (message.type === '_puppeteer_permission_request') {
          sawPermission = true
          child.stdin.write(JSON.stringify({
            type: 'permission_response',
            requestId: message.requestId,
            decision: 'deny',
            ...(origin ? { origin } : {}),
          }) + '\n')
        }
        if (message.type === '_puppeteer_error') {
          console.error(message.error)
        }
        if (message.type === '_puppeteer_log') {
          console.error(message.text)
        }
        if (message.type === 'user') {
          for (const item of message.message?.content ?? []) {
            if (item.type === 'tool_result' && item.is_error === true) {
              deniedResult = true
              setTimeout(finish, 250)
            }
          }
        }
        if (message.type === 'result') finish()
      }
    })

    child.stderr.on('data', (chunk) => process.stderr.write(chunk))
    child.on('exit', finish)
    child.stdin.write(JSON.stringify({
      type: 'prompt',
      text: 'Run the fixture command.',
    }) + '\n')

    setTimeout(finish, 10_000)
  })
}

const cases = [
  { origin: 'local', want: 'decline' },
  { origin: 'remote', want: 'cancel' },
  { origin: undefined, want: 'cancel' },
]
let pass = true
for (const { origin, want } of cases) {
  const r = await runCase(origin)
  const ok = r.sawPermission && r.deniedResult && !r.commandRan && r.decision === want
  if (!ok) pass = false
  console.log(JSON.stringify({ ...r, want, pass: ok }))
}
process.exit(pass ? 0 : 1)
