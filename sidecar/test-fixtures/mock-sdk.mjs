// Minimal stand-in for `query()` of @anthropic-ai/claude-agent-sdk, driven by
// MOCK_SCENARIO. Like the real SDK, it keeps the turn running after the input
// stream ends, so a tool can still ask for permission once stdin is closed.
//
//   permission  — one turn whose tool asks permission; reports the decision.
//   abort       — the CLI withdraws the request (signal aborts) after 200ms.
//   after-close — waits for the input to end, THEN a tool asks permission.
//   throw       — the session loop fails (like a CLI crash).
//   echo        — after init, echoes every later input message (`mock_input`).
//   context     — a turn per input; getContextUsage() answers like the real
//                 control channel, on the message stream: only once the
//                 consumer pulls the next message.

export function query({ prompt, options }) {
  const scenario = process.env.MOCK_SCENARIO || 'permission'
  const input = prompt[Symbol.asyncIterator]()

  async function ask(toolName) {
    const ac = new AbortController()
    if (scenario === 'abort') setTimeout(() => ac.abort(), 200)
    const res = await options.canUseTool(toolName, { file_path: 'out.txt' }, { signal: ac.signal, toolUseID: 'toolu_mock' })
    return { type: 'result', subtype: 'success', mock_tool: toolName, mock_behavior: res.behavior, mock_message: res.message ?? null }
  }

  const contextAnswers = []

  async function* run() {
    const first = await input.next() // the first user prompt
    if (scenario === 'throw') throw new Error('mock CLI crashed')
    // Echo the (JSON-safe) options and the first prompt so tests can assert what reached the SDK.
    yield { type: 'system', subtype: 'init', session_id: 'mock-session', model: 'mock', mock_options: JSON.parse(JSON.stringify(options)), mock_has_can_use_tool: typeof options.canUseTool === 'function', mock_first_input: first.value }
    if (scenario === 'echo') {
      for (let next = await input.next(); !next.done; next = await input.next()) {
        yield { type: 'mock_echo', mock_input: next.value }
      }
      return
    }
    if (scenario === 'context') {
      for (let turn = 0; ; turn++) {
        yield { type: 'result', subtype: 'success', mock_turn: turn }
        // The consumer pulled again: a control response sent meanwhile arrives.
        for (const answer of contextAnswers.splice(0)) answer(turn)
        if ((await input.next()).done) return
      }
    }
    if (scenario === 'after-close') {
      while (!(await input.next()).done) { /* drain follow-ups until stdin closes */ }
      yield await ask('Edit')
      return
    }
    yield await ask('Write')
    // Keep serving turns until the input stream ends.
    while (!(await input.next()).done) {
      yield { type: 'result', subtype: 'success' }
    }
  }

  const session = run()
  if (scenario === 'context') {
    session.getContextUsage = (opts) => {
      if (opts?.detail !== 'summary') return Promise.reject(new Error(`unexpected detail: ${opts?.detail}`))
      return new Promise((resolve) => contextAnswers.push((turn) => resolve(contextUsage(turn))))
    }
  }
  return session
}

// A getContextUsage() answer (SDK 0.3.319 shape), `messages` growing per turn.
function contextUsage(turn) {
  const row = { color: 'x', isFilled: true, categoryName: 'Messages', tokens: 1, percentage: 1, squareFullness: 1 }
  return {
    categories: [
      { id: 'system_prompt', name: 'System prompt', tokens: 3000, color: 'x', kind: 'used' },
      { id: 'system_tools', name: 'System tools', tokens: 12000, color: 'x', kind: 'used' },
      { id: 'mcp_tools', name: 'MCP tools', tokens: 9000, color: 'x', kind: 'used' },
      { id: 'mcp_tools_deferred', name: 'MCP tools (deferred)', tokens: 20000, color: 'x', kind: 'deferred', isDeferred: true },
      { id: 'memory_files', name: 'Memory files', tokens: 1500, color: 'x', kind: 'used' },
      { name: 'Legacy row', tokens: 7, color: 'x', kind: 'used' },
      { id: 'messages', name: 'Messages', tokens: 40000 + turn * 1000, color: 'x', kind: 'used' },
      { id: 'free_space', name: 'Free space', tokens: 100000, color: 'x', kind: 'free' },
    ],
    totalTokens: 65500 + turn * 1000,
    maxTokens: 200000,
    rawMaxTokens: 200000,
    percentage: 33,
    gridRows: [[row, row], [row]],
    model: 'claude-opus-5-5',
    memoryFiles: [
      { path: '/p/CLAUDE.md', type: 'Project', tokens: 1000 },
      { path: '/u/CLAUDE.md', type: 'User', tokens: 500 },
    ],
    mcpTools: [10, 900, 50, 4000, 300, 2000, 70].map((tokens, i) => ({ name: `mcp__s__t${i}`, serverName: 's', tokens, isLoaded: i !== 3 })),
    agents: [{ agentType: 'a', source: 'user', tokens: 5 }],
    skills: { totalSkills: 1, includedSkills: 1, tokens: 5, skillFrontmatter: [{ name: 'k', source: 'user', tokens: 5 }] },
    isAutoCompactEnabled: true,
    messageBreakdown: {
      toolCallTokens: 5000, toolResultTokens: 30000, attachmentTokens: 0, assistantMessageTokens: 3000, userMessageTokens: 2000,
      redirectedContextTokens: 0, unattributedTokens: 0, toolCallsByType: [], attachmentsByType: [],
    },
    apiUsage: null,
  }
}
