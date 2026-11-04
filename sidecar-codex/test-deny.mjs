// Regression test: a denied Codex command must never execute.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { existsSync, mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const work = mkdtempSync(join(tmpdir(), 'puppeteer-codex-deny-'))
const marker = join(work, 'DENIED_COMMAND_RAN')
const fixture = join(here, 'test-fixture-app-server.mjs')
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
  },
})

let buffer = ''
let sawPermission = false
let deniedResult = false
let finished = false

function finish(code) {
  if (finished) return
  finished = true
  try { child.kill() } catch {}
  const commandRan = existsSync(marker)
  const pass = sawPermission && deniedResult && !commandRan
  console.log(JSON.stringify({ sawPermission, deniedResult, commandRan, pass }))
  process.exit(pass ? 0 : (code || 1))
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
          setTimeout(() => finish(0), 250)
        }
      }
    }
    if (message.type === 'result') finish(0)
  }
})

child.stderr.on('data', (chunk) => process.stderr.write(chunk))
child.on('exit', (code) => finish(code ?? 1))
child.stdin.write(JSON.stringify({
  type: 'prompt',
  text: 'Run the fixture command.',
}) + '\n')

setTimeout(() => finish(1), 10_000)
