#!/usr/bin/env node
// Deterministic sidecar tests (no CLI, no network): the Agent SDK is replaced by
// test-fixtures/mock-sdk.mjs via a module hook. Covers the permission channel's
// no-hang guarantees and the liveness protocol.
//
// Run: node test-mocked.mjs   (from the sidecar/ dir)

import { spawn } from 'node:child_process'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { dirname, join } from 'node:path'
import assert from 'node:assert/strict'

const here = dirname(fileURLToPath(import.meta.url))

// Spawn the sidecar under `scenario`; `onMessage(msg, ctl)` drives it. Resolves
// with every message once the process exits (or rejects on timeout = a hang).
function runSidecar(scenario, onMessage, timeoutMs = 10_000, extraEnv = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      // A URL: `--import` reads a Windows path (C:\...) as a URL with scheme `c:`.
      ['--import', pathToFileURL(join(here, 'test-fixtures', 'mock-sdk-register.mjs')).href, join(here, 'index.mjs')],
      {
        cwd: here,
        env: { ...process.env, MOCK_SCENARIO: scenario, PUPPETEER_SIDECAR_HEARTBEAT_MS: '100', ...extraEnv },
        stdio: ['pipe', 'pipe', 'inherit'],
      },
    )
    const messages = []
    const ctl = {
      send: (obj) => child.stdin.write(JSON.stringify(obj) + '\n'),
      closeInput: () => child.stdin.end(),
    }
    const timer = setTimeout(() => {
      child.kill('SIGKILL')
      reject(new Error(`[${scenario}] sidecar did not exit within ${timeoutMs}ms (hang). Saw: ${messages.map((m) => m.type).join(', ')}`))
    }, timeoutMs)
    let buf = ''
    child.stdout.on('data', (chunk) => {
      buf += chunk.toString()
      let i
      while ((i = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, i)
        buf = buf.slice(i + 1)
        if (!line.trim()) continue
        const msg = JSON.parse(line)
        messages.push(msg)
        onMessage(msg, ctl, messages)
      }
    })
    child.on('exit', (code) => {
      clearTimeout(timer)
      resolve({ messages, code })
    })
  })
}

const startPrompt = (ctl) => ctl.send({ type: 'prompt', text: 'go', options: { permissionMode: 'default' } })
const of = (messages, type) => messages.filter((m) => m.type === type)

const tests = {
  // Liveness protocol: ready declares the heartbeat, and heartbeats flow.
  async 'ready declares heartbeat capability and heartbeats arrive'() {
    const { messages } = await runSidecar('permission', (msg, ctl, all) => {
      if (msg.type === '_puppeteer_ready') startPrompt(ctl)
      if (msg.type === '_puppeteer_permission_request') {
        // Hold the answer until a couple of heartbeats went by.
        const wait = setInterval(() => {
          if (of(all, '_puppeteer_heartbeat').length >= 2) {
            clearInterval(wait)
            ctl.send({ type: 'permission_response', requestId: msg.requestId, decision: 'allow' })
            ctl.closeInput()
          }
        }, 20)
      }
    })
    const ready = of(messages, '_puppeteer_ready')[0]
    assert.equal(ready.capabilities?.heartbeat_ms, 100)
    assert.ok(of(messages, '_puppeteer_heartbeat').length >= 2)
    assert.equal(of(messages, 'result')[0].mock_behavior, 'allow')
  },

  // Control: a normal answer is delivered and the process exits on input close.
  async 'answered request is delivered; session closes and exits'() {
    const { messages, code } = await runSidecar('permission', (msg, ctl) => {
      if (msg.type === '_puppeteer_ready') startPrompt(ctl)
      if (msg.type === '_puppeteer_permission_request') {
        ctl.send({ type: 'permission_response', requestId: msg.requestId, decision: 'deny', reason: 'no thanks' })
        ctl.closeInput()
      }
    })
    const result = of(messages, 'result')[0]
    assert.equal(result.mock_behavior, 'deny')
    assert.equal(result.mock_message, 'no thanks')
    assert.equal(of(messages, '_puppeteer_permission_cancelled').length, 0)
    assert.equal(of(messages, '_puppeteer_closed').length, 1)
    assert.equal(of(messages, '_puppeteer_closed')[0].code, 0)
    assert.equal(code, 0)
  },

  // A failed session reports it on the close line AND in the exit code, so the
  // broker marks the run failed even if it stops waiting on the pipes.
  async 'failed session closes with code 1 and exits 1'() {
    const { messages, code } = await runSidecar('throw', (msg, ctl) => {
      if (msg.type === '_puppeteer_ready') startPrompt(ctl)
    })
    assert.equal(of(messages, '_puppeteer_error').length, 1)
    assert.equal(of(messages, '_puppeteer_closed')[0].code, 1)
    assert.equal(code, 1)
  },

  // The original hang: stdin closes while a request is open. The sidecar must
  // settle it itself (deny + cancelled), finish the turn, close and exit.
  async 'input closed while a request is open → denied, cancelled, exits'() {
    let requestId = null
    const { messages, code } = await runSidecar('permission', (msg, ctl) => {
      if (msg.type === '_puppeteer_ready') startPrompt(ctl)
      if (msg.type === '_puppeteer_permission_request') {
        requestId = msg.requestId
        ctl.closeInput() // never answer
      }
    })
    const cancelled = of(messages, '_puppeteer_permission_cancelled')
    assert.equal(cancelled.length, 1)
    assert.equal(cancelled[0].requestId, requestId)
    assert.equal(cancelled[0].level, 'warn')
    const result = of(messages, 'result')[0]
    assert.equal(result.mock_behavior, 'deny')
    assert.match(result.mock_message, /input channel closed/)
    assert.equal(of(messages, '_puppeteer_closed').length, 1)
    assert.equal(code, 0)
  },

  // A tool asking AFTER stdin closed is denied at once — no prompt is raised
  // that nobody could ever answer.
  async 'request after input closed is denied immediately, no prompt'() {
    const { messages, code } = await runSidecar('after-close', (msg, ctl) => {
      if (msg.type === '_puppeteer_ready') {
        startPrompt(ctl)
        ctl.closeInput()
      }
    })
    assert.equal(of(messages, '_puppeteer_permission_request').length, 0)
    const result = of(messages, 'result')[0]
    assert.equal(result.mock_tool, 'Edit')
    assert.equal(result.mock_behavior, 'deny')
    assert.ok(of(messages, '_puppeteer_log').some((m) => m.level === 'warn' && /Edit was denied/.test(m.text)))
    assert.equal(code, 0)
  },

  // The CLI withdraws the request (interrupt / tool aborted): the prompt is
  // dropped instead of lingering forever.
  async 'CLI-withdrawn request is cancelled (info) and settled'() {
    let requestId = null
    const { messages } = await runSidecar('abort', (msg, ctl) => {
      if (msg.type === '_puppeteer_ready') startPrompt(ctl)
      if (msg.type === '_puppeteer_permission_request') requestId = msg.requestId
      if (msg.type === 'result') ctl.closeInput()
    })
    const cancelled = of(messages, '_puppeteer_permission_cancelled')
    assert.equal(cancelled.length, 1)
    assert.equal(cancelled[0].requestId, requestId)
    assert.equal(cancelled[0].level, 'info')
    assert.equal(of(messages, 'result')[0].mock_behavior, 'deny')
  },

  // Translation engine: a no-tools session strips every tool source and denies
  // any call without a prompt — even with looser options and MCP env fallbacks.
  async 'noTools session has no tools and denies every call without a prompt'() {
    const { messages, code } = await runSidecar('permission', (msg, ctl) => {
      if (msg.type === '_puppeteer_ready') {
        ctl.send({
          type: 'prompt',
          text: 'go',
          options: {
            noTools: true,
            permissionMode: 'bypassPermissions',
            allowedTools: ['Bash', 'mcp__puppeteer'],
            mcpServers: { puppeteer: { type: 'http', url: 'http://127.0.0.1:1/mcp' } },
          },
        })
      }
      if (msg.type === 'result') ctl.closeInput()
    }, 10_000, { PUPPETEER_MCP_SERVERS: JSON.stringify({ env: { type: 'stdio', command: 'x' } }), PUPPETEER_ALLOWED_TOOLS: '["Bash"]' })
    const init = messages.find((m) => m.type === 'system' && m.subtype === 'init')
    const o = init.mock_options
    assert.deepEqual(o.tools, [])
    assert.deepEqual(o.allowedTools, [])
    assert.deepEqual(o.mcpServers, {})
    assert.deepEqual(o.skills, [])
    assert.equal(o.strictMcpConfig, true)
    assert.equal(o.permissionMode, 'dontAsk')
    // No user/project settings, hooks or CLAUDE.md, and a short prompt of its
    // own instead of the coding preset (AC-12).
    assert.deepEqual(o.settingSources, [])
    assert.equal(o.systemPrompt.type, 'custom')
    assert.ok(o.systemPrompt.prompt.length < 300, 'side-task system prompt is not short')
    assert.equal(init.mock_has_can_use_tool, true)
    assert.equal(of(messages, '_puppeteer_permission_request').length, 0)
    const result = of(messages, 'result')[0]
    assert.equal(result.mock_tool, 'Write')
    assert.equal(result.mock_behavior, 'deny')
    assert.match(result.mock_message, /no tool access/)
    assert.equal(code, 0)
  },

  // Control: a regular session keeps the default tool set.
  async 'regular session does not restrict tools'() {
    const { messages } = await runSidecar('permission', (msg, ctl) => {
      if (msg.type === '_puppeteer_ready') startPrompt(ctl)
      if (msg.type === '_puppeteer_permission_request') {
        ctl.send({ type: 'permission_response', requestId: msg.requestId, decision: 'allow' })
        ctl.closeInput()
      }
    })
    const o = messages.find((m) => m.type === 'system' && m.subtype === 'init').mock_options
    assert.equal(o.tools, undefined)
    assert.equal(o.strictMcpConfig, undefined)
    assert.equal(o.settingSources, undefined)
    assert.equal(o.systemPrompt.type, 'preset')
    assert.equal(o.permissionMode, 'default')
    // Secret store reads are denied in every session, whatever the OS.
    for (const rule of [
      'Bash(security find-generic-password:*)',
      'Bash(secret-tool:*)',
      'Bash(kwallet-query:*)',
      'Bash(gdbus *org.freedesktop.secrets*)',
      'Bash(dbus-send *org.kde.kwalletd*)',
      'Bash(cmdkey.exe:*)',
      'PowerShell(vaultcmd:*)',
      'PowerShell(*Get-StoredCredential*)',
      'PowerShell(*CredRead*)',
      'Bash(*PasswordVault*)',
    ]) assert.ok(o.disallowedTools.includes(rule), rule)
    assert.equal(o.resume, undefined)
    assert.equal(o.resumeSessionAt, undefined)
  },

  // Rewind: a resumed session goes on from the entry the broker names.
  async 'resume after a rewind continues from the given entry'() {
    const rewound = await runSidecar('permission', (msg, ctl) => {
      if (msg.type === '_puppeteer_ready') ctl.send({ type: 'prompt', text: 'again' })
      if (msg.type === '_puppeteer_permission_request') {
        ctl.send({ type: 'permission_response', requestId: msg.requestId, decision: 'allow' })
        ctl.closeInput()
      }
    }, 10_000, { PUPPETEER_RESUME_SESSION: 'sess-1', PUPPETEER_RESUME_AT: 'entry-7' })
    const o = rewound.messages.find((m) => m.type === 'system' && m.subtype === 'init').mock_options
    assert.equal(o.resume, 'sess-1')
    assert.equal(o.resumeSessionAt, 'entry-7')

    // Without a session to resume there is nothing to go back in.
    const fresh = await runSidecar('permission', (msg, ctl) => {
      if (msg.type === '_puppeteer_ready') startPrompt(ctl)
      if (msg.type === '_puppeteer_permission_request') {
        ctl.send({ type: 'permission_response', requestId: msg.requestId, decision: 'allow' })
        ctl.closeInput()
      }
    }, 10_000, { PUPPETEER_RESUME_AT: 'entry-7' })
    const f = fresh.messages.find((m) => m.type === 'system' && m.subtype === 'init').mock_options
    assert.equal(f.resumeSessionAt, undefined)
  },

  // Fork (UX-28): the resume copies the session (forkSession) up to the kept
  // entry; nothing forks without a session to resume.
  async 'a fork resumes a copy of the session up to the kept entry'() {
    const run = (env) => runSidecar('permission', (msg, ctl) => {
      if (msg.type === '_puppeteer_ready') ctl.send({ type: 'prompt', text: 'again' })
      if (msg.type === '_puppeteer_permission_request') {
        ctl.send({ type: 'permission_response', requestId: msg.requestId, decision: 'allow' })
        ctl.closeInput()
      }
    }, 10_000, env)
    const initOf = ({ messages }) => messages.find((m) => m.type === 'system' && m.subtype === 'init').mock_options
    const forked = initOf(await run({ PUPPETEER_RESUME_SESSION: 'sess-1', PUPPETEER_RESUME_AT: 'entry-7', PUPPETEER_FORK_SESSION: '1' }))
    assert.equal(forked.resume, 'sess-1')
    assert.equal(forked.resumeSessionAt, 'entry-7')
    assert.equal(forked.forkSession, true)
    // A plain resume goes on in the session itself.
    const plain = initOf(await run({ PUPPETEER_RESUME_SESSION: 'sess-1' }))
    assert.equal(plain.forkSession, undefined)
    const fresh = initOf(await run({ PUPPETEER_FORK_SESSION: '1' }))
    assert.equal(fresh.resume, undefined)
    assert.equal(fresh.forkSession, undefined)
  },

  // Effort (UX-16): reaches the SDK only when the model takes that level; a
  // resume (first prompt without options) gets it from the broker's env.
  async 'effort reaches the SDK only when the model supports it'() {
    const initOptions = async (options, env = {}) => {
      const { messages } = await runSidecar('permission', (msg, ctl) => {
        if (msg.type === '_puppeteer_ready') ctl.send({ type: 'prompt', text: 'go', ...(options ? { options } : {}) })
        if (msg.type === '_puppeteer_permission_request') {
          ctl.send({ type: 'permission_response', requestId: msg.requestId, decision: 'allow' })
          ctl.closeInput()
        }
      }, 10_000, env)
      return messages.find((m) => m.type === 'system' && m.subtype === 'init').mock_options
    }
    const base = { permissionMode: 'default' }
    assert.equal((await initOptions({ ...base, model: 'claude-opus-5-5', effort: 'high' })).effort, 'high')
    // Engine default model: the CLI decides, so the level goes through.
    assert.equal((await initOptions({ ...base, effort: 'max' })).effort, 'max')
    // No effort on this model at all (dated / Bedrock spellings included).
    assert.equal((await initOptions({ ...base, model: 'claude-haiku-4-5-20251001', effort: 'low' })).effort, undefined)
    assert.equal((await initOptions({ ...base, model: 'us.anthropic.claude-sonnet-4-5-20250929-v1:0', effort: 'low' })).effort, undefined)
    // The model takes effort but not this level.
    assert.equal((await initOptions({ ...base, model: 'claude-sonnet-4-6[1m]', effort: 'xhigh' })).effort, undefined)
    assert.equal((await initOptions({ ...base, model: 'claude-sonnet-4-6[1m]', effort: 'max' })).effort, 'max')
    assert.equal((await initOptions({ ...base, model: 'claude-opus-5-5', effort: 'turbo' })).effort, undefined)
    assert.equal((await initOptions({ ...base, model: 'claude-opus-5-5' })).effort, undefined)
    // Resume: no options on the prompt, model + effort from env.
    const resumed = await initOptions(null, { PUPPETEER_RESUME_SESSION: 'sess-1', PUPPETEER_MODEL: 'claude-opus-4-8', PUPPETEER_EFFORT: 'xhigh' })
    assert.equal(resumed.effort, 'xhigh')
    const resumedHaiku = await initOptions(null, { PUPPETEER_RESUME_SESSION: 'sess-1', PUPPETEER_MODEL: 'claude-haiku-4-5', PUPPETEER_EFFORT: 'xhigh' })
    assert.equal(resumedHaiku.effort, undefined)
  },

  // Fallback model (AC-17): the broker's env reaches the SDK on start and on
  // resume, except when it is the model itself (the SDK throws on that) or for
  // a tool-less side task.
  async 'fallback model reaches the SDK unless it is the model or a side task'() {
    const initOptions = async (options, env = {}) => {
      const { messages } = await runSidecar('echo', (msg, ctl) => {
        if (msg.type === '_puppeteer_ready') ctl.send({ type: 'prompt', text: 'go', ...(options ? { options } : {}) })
        if (msg.type === 'system' && msg.subtype === 'init') ctl.closeInput()
      }, 10_000, env)
      return messages.find((m) => m.type === 'system' && m.subtype === 'init').mock_options
    }
    const base = { permissionMode: 'default' }
    const env = { PUPPETEER_FALLBACK_MODEL: 'sonnet' }
    assert.equal((await initOptions({ ...base, model: 'claude-opus-5-5' }, env)).fallbackModel, 'sonnet')
    // Engine default model: the CLI resolves it, the fallback still applies.
    assert.equal((await initOptions(base, env)).fallbackModel, 'sonnet')
    const resumed = await initOptions(null, { ...env, PUPPETEER_RESUME_SESSION: 'sess-1', PUPPETEER_MODEL: 'claude-opus-4-8' })
    assert.equal(resumed.resume, 'sess-1')
    assert.equal(resumed.fallbackModel, 'sonnet')
    // The same id as the model: none.
    assert.equal((await initOptions({ ...base, model: 'sonnet' }, env)).fallbackModel, undefined)
    assert.equal((await initOptions(null, { ...env, PUPPETEER_MODEL: 'sonnet' })).fallbackModel, undefined)
    // A side task keeps its own model.
    assert.equal((await initOptions({ ...base, model: 'haiku', noTools: true }, env)).fallbackModel, undefined)
    // Turned off in Settings: the broker sets no env.
    assert.equal((await initOptions({ ...base, model: 'claude-opus-5-5' })).fallbackModel, undefined)
  },

  // Unattended turn (AC-23): the broker's PUPPETEER_MAX_TURNS reaches the SDK
  // as `maxTurns` (not for a tool-less side task), and a turn it ends is
  // reported with `_puppeteer_limit_reached` right before its result.
  async 'max turns reach the SDK from the env and a turn they end is reported'() {
    const run = (env, options = { permissionMode: 'default' }) => runSidecar('max-turns', (msg, ctl) => {
      if (msg.type === '_puppeteer_ready') ctl.send({ type: 'prompt', text: 'go', options })
      if (msg.type === 'result') ctl.closeInput()
    }, 10_000, env)
    const limited = await run({ PUPPETEER_MAX_TURNS: '200' })
    const init = limited.messages.find((m) => m.type === 'system' && m.subtype === 'init')
    assert.equal(init.mock_options.maxTurns, 200)
    const [limit] = of(limited.messages, '_puppeteer_limit_reached')
    assert.deepEqual(limit, { type: '_puppeteer_limit_reached', kind: 'turns', limit: 200 })
    const [result] = of(limited.messages, 'result')
    assert.equal(result.subtype, 'error_max_turns')
    assert.ok(limited.messages.indexOf(limit) < limited.messages.indexOf(result))
    assert.equal(limited.code, 0)

    // No env (an attended turn), an off value, a side task: no limit, no report.
    for (const [env, options] of [
      [{}, undefined],
      [{ PUPPETEER_MAX_TURNS: '0' }, undefined],
      [{ PUPPETEER_MAX_TURNS: 'many' }, undefined],
      [{ PUPPETEER_MAX_TURNS: '200' }, { permissionMode: 'default', noTools: true }],
    ]) {
      const { messages } = await run(env, options)
      const opts = messages.find((m) => m.type === 'system' && m.subtype === 'init').mock_options
      assert.equal(opts.maxTurns, undefined, JSON.stringify(env))
      assert.equal(of(messages, '_puppeteer_limit_reached').length, 0)
      assert.equal(of(messages, 'result')[0].subtype, 'success')
    }
  },

  // Response style (UX-17): the broker composes one append (account context,
  // framing, style). It reaches the SDK on the preset prompt, and on a resume
  // (first prompt without options) from PUPPETEER_APPEND_SYSTEM_PROMPT — the
  // first turn's text as it is (AC-06), recorded by the SDK (`snapshot`).
  async 'response style reaches the appended system prompt, also on resume'() {
    const style = '<response-style>\nBe short.\n</response-style>'
    const append = `ACCOUNT\n\nFRAMING\n\n${style}`
    const initOf = async (options, env = {}) => {
      const { messages } = await runSidecar('permission', (msg, ctl) => {
        if (msg.type === '_puppeteer_ready') ctl.send({ type: 'prompt', text: 'go', ...(options ? { options } : {}) })
        if (msg.type === '_puppeteer_permission_request') {
          ctl.send({ type: 'permission_response', requestId: msg.requestId, decision: 'allow' })
          ctl.closeInput()
        }
      }, 10_000, env)
      return messages.find((m) => m.type === 'system' && m.subtype === 'init')
    }
    const started = (await initOf({ permissionMode: 'default', appendSystemPrompt: append })).mock_options
    assert.deepEqual(started.systemPrompt, { type: 'preset', preset: 'claude_code', snapshot: true, append })
    const resumed = (await initOf(null, { PUPPETEER_RESUME_SESSION: 'sess-1', PUPPETEER_APPEND_SYSTEM_PROMPT: append })).mock_options
    assert.equal(resumed.resume, 'sess-1')
    assert.deepEqual(resumed.systemPrompt, started.systemPrompt)
    // Nothing to append: still the recorded preset.
    const plain = (await initOf({ permissionMode: 'default' })).mock_options
    assert.deepEqual(plain.systemPrompt, { type: 'preset', preset: 'claude_code', snapshot: true })

    // A style changed since: a block before the first message; the system
    // prompt stays as it was. Only the first message gets it.
    const seed = '<session-framing>\nThe response style of this session changed.\n</session-framing>'
    const seeded = await initOf(null, { PUPPETEER_RESUME_SESSION: 'sess-1', PUPPETEER_APPEND_SYSTEM_PROMPT: append, PUPPETEER_SEED_STYLE: seed })
    assert.deepEqual(seeded.mock_options.systemPrompt, started.systemPrompt)
    assert.equal(seeded.mock_first_input.message.content, `${seed}\n\ngo`)
    assert.equal((await initOf(null, { PUPPETEER_RESUME_SESSION: 'sess-1' })).mock_first_input.message.content, 'go')
    const { messages } = await runSidecar('echo', (msg, ctl) => {
      if (msg.type === '_puppeteer_ready') ctl.send({ type: 'prompt', text: 'one' })
      if (msg.type === 'system' && msg.subtype === 'init') {
        ctl.send({ type: 'prompt', text: 'two' })
        ctl.closeInput()
      }
    }, 10_000, { PUPPETEER_SEED_STYLE: seed })
    assert.deepEqual(of(messages, 'mock_echo').map((m) => m.mock_input.message.content), ['two'])
  },

  // Context measurement (AC-08): after each turn's result, the window's makeup
  // as a summary (no grid, no per-skill lists). The SDK answers on the message
  // stream, so the loop must not wait for it: the second turn still runs, and
  // each answer arrives with the turn after it.
  async 'each result reports a context usage summary without blocking the loop'() {
    const { messages, code } = await runSidecar('context', (msg, ctl) => {
      if (msg.type === '_puppeteer_ready') startPrompt(ctl)
      if (msg.type === 'result' && msg.mock_turn === 0) ctl.send({ type: 'prompt', text: 'again' })
      if (msg.type === 'result' && msg.mock_turn === 1) ctl.closeInput()
    })
    assert.equal(code, 0)
    assert.equal(of(messages, 'result').length, 2)
    const usage = of(messages, '_puppeteer_context_usage')
    assert.equal(usage.length, 2)
    assert.deepEqual(usage[0], {
      type: '_puppeteer_context_usage',
      model: 'claude-opus-5-5',
      totalTokens: 65500,
      maxTokens: 200000,
      categories: {
        system_prompt: 3000, system_tools: 12000, mcp_tools: 9000, mcp_tools_deferred: 20000,
        memory_files: 1500, 'Legacy row': 7, messages: 40000, free_space: 100000,
      },
      mcpToolCount: 7,
      mcpTools: [
        { name: 'mcp__s__t3', serverName: 's', tokens: 4000, isLoaded: false },
        { name: 'mcp__s__t5', serverName: 's', tokens: 2000, isLoaded: true },
        { name: 'mcp__s__t1', serverName: 's', tokens: 900, isLoaded: true },
        { name: 'mcp__s__t4', serverName: 's', tokens: 300, isLoaded: true },
        { name: 'mcp__s__t6', serverName: 's', tokens: 70, isLoaded: true },
      ],
      memoryFiles: { count: 2, tokens: 1500 },
      messageBreakdown: { toolCallTokens: 5000, toolResultTokens: 30000, attachmentTokens: 0, assistantMessageTokens: 3000, userMessageTokens: 2000 },
    })
    assert.equal(usage[1].categories.messages, 41000)
    // Answered after the turn's result, and flushed before the session closes.
    const at = (m) => messages.indexOf(m)
    const results = of(messages, 'result')
    assert.ok(at(usage[0]) > at(results[0]) && at(usage[1]) > at(results[1]))
    assert.ok(at(usage[1]) < at(of(messages, '_puppeteer_closed')[0]))
    assert.ok(!of(messages, '_puppeteer_log').some((m) => /context usage capture skipped/.test(m.text)))
  },

  // Steer (D4-06): a steer reaches the SDK as a user message with priority
  // `next` (folded into the running turn); a plain follow-up goes with `later`
  // (D4-15: a turn of its own, after the one in flight). Before any turn there
  // is nothing to steer: a warning, no crash. Only the steer that reached the
  // SDK is reported back (`_puppeteer_steered`), for the broker to log (R-13).
  async 'steer reaches the SDK with priority next; a follow-up with later'() {
    const img = { media_type: 'image/png', data: 'AAAA' }
    const { messages, code } = await runSidecar('echo', (msg, ctl) => {
      if (msg.type === '_puppeteer_ready') {
        ctl.send({ type: 'steer', text: 'too early' })
        startPrompt(ctl)
      }
      if (msg.type === 'system' && msg.subtype === 'init') {
        ctl.send({ type: 'steer', text: 'focus on the tests', images: [img] })
        ctl.send({ type: 'prompt', text: 'then this' })
        ctl.closeInput()
      }
    })
    const early = of(messages, '_puppeteer_log').filter((m) => m.level === 'warn' && /Steer not applied/.test(m.text))
    assert.equal(early.length, 1)
    assert.deepEqual(of(messages, '_puppeteer_steered').map(({ text, images }) => ({ text, images })), [
      { text: 'focus on the tests', images: [img] },
    ])
    const [steer, followUp] = of(messages, 'mock_echo').map((m) => m.mock_input)
    assert.equal(steer.priority, 'next')
    assert.deepEqual(steer.message.content, [
      { type: 'text', text: 'focus on the tests' },
      { type: 'image', source: { type: 'base64', media_type: 'image/png', data: 'AAAA' } },
    ])
    assert.equal(followUp.priority, 'later')
    assert.equal(followUp.message.content, 'then this')
    assert.equal(code, 0)
  },
}

let failed = 0
for (const [name, fn] of Object.entries(tests)) {
  try {
    await fn()
    console.log(`ok   ${name}`)
  } catch (e) {
    failed++
    console.log(`FAIL ${name}\n     ${e.message}`)
  }
}
console.log(failed ? `\n${failed} failed` : `\nall ${Object.keys(tests).length} passed`)
process.exit(failed ? 1 : 0)
