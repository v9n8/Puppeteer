// Regression test: an MCP tool approval Codex asks for follows the permission
// mode. Bypass accepts it and Auto declines it, both without a prompt; a sign-in
// URL, a real question among the approvals, and Manual still ask the user.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { existsSync, mkdtempSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import assert from 'node:assert/strict'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-app-server.mjs')

function runCase(mode, env) {
  return new Promise((resolve) => {
    const work = mkdtempSync(join(tmpdir(), 'puppeteer-codex-mcp-mode-'))
    const replyFile = join(work, 'reply')
    const childEnv = { ...process.env }
    delete childEnv.PUPPETEER_RESUME_SESSION
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...childEnv, PUPPETEER_PERMISSION_MODE: mode, PUPPETEER_CODEX_PATH: fixture, PUPPETEER_DECISION_OUT: replyFile, ...env },
    })

    let buffer = ''
    let asked = false
    const logs = []
    let finished = false
    const finish = () => {
      if (finished) return
      finished = true
      try { child.kill() } catch {}
      const reply = existsSync(replyFile) ? JSON.parse(readFileSync(replyFile, 'utf8')) : null
      resolve({ asked, reply, logs })
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
        if (message.type === '_puppeteer_log') logs.push(message.text)
        if (message.type === '_puppeteer_permission_request') {
          asked = true
          child.stdin.write(JSON.stringify({ type: 'permission_response', requestId: message.requestId, decision: 'deny', origin: 'local' }) + '\n')
        }
        if (message.type === 'result') setTimeout(finish, 100)
      }
    })
    child.stderr.on('data', (chunk) => process.stderr.write(chunk))
    child.on('exit', finish)
    child.stdin.write(JSON.stringify({ type: 'prompt', text: 'Use the tool.' }) + '\n')
    setTimeout(finish, 10_000)
  })
}

const elicitation = { PUPPETEER_MCP_PERMISSION_KEY_TEST: '1' }
const approvalQuestion = { PUPPETEER_FIXTURE_USER_INPUT: '1', PUPPETEER_FIXTURE_APPROVAL_ONLY: '1' }

let r = await runCase('bypassPermissions', elicitation)
assert.equal(r.asked, false)
assert.equal(r.reply?.action, 'accept')
console.log('ok   bypass accepts an MCP elicitation approval without a prompt')

r = await runCase('auto', elicitation)
assert.equal(r.asked, false)
assert.equal(r.reply?.action, 'decline')
assert.ok(r.logs.some((t) => /declined: Auto mode does not ask/.test(t)))
console.log('ok   auto declines an MCP elicitation approval without a prompt')

r = await runCase('bypassPermissions', { ...elicitation, PUPPETEER_MCP_ELICITATION_MODE: 'url' })
assert.equal(r.asked, true)
console.log('ok   a sign-in URL still asks in bypass')

r = await runCase('default', elicitation)
assert.equal(r.asked, true)
console.log('ok   manual still asks')

r = await runCase('bypassPermissions', approvalQuestion)
assert.equal(r.asked, false)
assert.deepEqual(r.reply, { answers: { 'mcp_tool_call_approval_call-1': { answers: ['Allow'] } } })
console.log('ok   bypass answers an MCP approval question with Allow')

r = await runCase('auto', approvalQuestion)
assert.equal(r.asked, false)
assert.deepEqual(r.reply, { answers: {} })
console.log('ok   auto declines an MCP approval question')

r = await runCase('bypassPermissions', { PUPPETEER_FIXTURE_USER_INPUT: '1' })
assert.equal(r.asked, true)
console.log('ok   a real question among the approvals still asks')
