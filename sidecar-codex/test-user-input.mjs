// Regression test: codex's request_user_input reaches the user as an
// AskUserQuestion prompt (its MCP tool approval questions included, with the
// options codex sent), and the answers go back keyed by question id. A deny
// answers nothing, as before.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { existsSync, mkdtempSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import assert from 'node:assert/strict'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-app-server.mjs')

function runCase(onPermission) {
  return new Promise((resolve) => {
    const work = mkdtempSync(join(tmpdir(), 'puppeteer-codex-user-input-'))
    const replyFile = join(work, 'reply')
    const childEnv = { ...process.env }
    delete childEnv.PUPPETEER_RESUME_SESSION
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: {
        ...childEnv,
        PUPPETEER_PERMISSION_MODE: 'default',
        PUPPETEER_CODEX_PATH: fixture,
        PUPPETEER_FIXTURE_USER_INPUT: '1',
        PUPPETEER_DECISION_OUT: replyFile,
      },
    })

    let buffer = ''
    let request = null
    let finished = false
    const finish = () => {
      if (finished) return
      finished = true
      try { child.kill() } catch {}
      const reply = existsSync(replyFile) ? JSON.parse(readFileSync(replyFile, 'utf8')) : null
      resolve({ request, reply })
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
          request = message
          child.stdin.write(JSON.stringify({ type: 'permission_response', requestId: message.requestId, ...onPermission(message) }) + '\n')
        }
        if (message.type === '_puppeteer_error') console.error(message.error)
        if (message.type === 'result') setTimeout(finish, 100)
      }
    })
    child.stderr.on('data', (chunk) => process.stderr.write(chunk))
    child.on('exit', finish)
    child.stdin.write(JSON.stringify({ type: 'prompt', text: 'Ask me.' }) + '\n')
    setTimeout(finish, 10_000)
  })
}

// Answered: the prompt is Claude's AskUserQuestion shape, the reply is by id.
const answered = await runCase(() => ({
  decision: 'allow',
  origin: 'local',
  answers: { 'Which color?': 'Blue', 'Allow Drive to run tool "list_files"?': 'Allow for this session' },
}))
assert.ok(answered.request, 'no permission request was raised')
assert.equal(answered.request.tool_name, 'AskUserQuestion')
assert.deepEqual(answered.request.tool_input.questions, [
  {
    header: 'Theme',
    question: 'Which color?',
    multiSelect: false,
    options: [{ label: 'Red', description: 'Warm' }, { label: 'Blue', description: 'Cool' }],
  },
  {
    header: 'Approve app tool call?',
    question: 'Allow Drive to run tool "list_files"?',
    multiSelect: false,
    options: [
      { label: 'Allow', description: 'Run the tool and continue.' },
      { label: 'Allow for this session', description: 'Run the tool and remember this choice for this session.' },
      { label: 'Cancel', description: 'Cancel this tool call' },
    ],
  },
])
assert.deepEqual(answered.reply, {
  answers: {
    color: { answers: ['Blue'] },
    'mcp_tool_call_approval_call-1': { answers: ['Allow for this session'] },
  },
})
console.log('ok   request_user_input → AskUserQuestion, answers by question id')

// Denied (Esc / Deny on the question): no answers.
const denied = await runCase(() => ({ decision: 'deny', origin: 'local' }))
assert.ok(denied.request, 'no permission request was raised')
assert.deepEqual(denied.reply, { answers: {} })
console.log('ok   a denied question answers nothing')
