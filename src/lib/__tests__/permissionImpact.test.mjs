// Unit tests for lib/permissionImpact.ts (permission window Purpose / Impact, V3-31).
// Run: node --test src/lib/__tests__/permissionImpact.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  classifyPermissionRisk,
  splitShellCommands,
  permissionImpact,
  permissionPurpose,
  permissionDiffParts,
  permissionHeadline,
  permissionRiskNotes,
  impactOpensItself,
} from '../permissionImpact.ts'

const bash = (command, extra = {}) => ({ tool_name: 'Bash', tool_input: { command, ...extra }, cwd: '/w' })
const risks = (req) => classifyPermissionRisk(req)

test('risk: read tools are Read', () => {
  for (const n of ['Read', 'Grep', 'Glob', 'LS']) assert.deepEqual(risks({ tool_name: n, tool_input: {} }), ['read'])
})

test('risk: edit tools are Write, including Codex apply_patch', () => {
  for (const n of ['Edit', 'Write', 'MultiEdit', 'NotebookEdit', 'apply_patch']) {
    assert.deepEqual(risks({ tool_name: n, tool_input: {} }), ['write'])
  }
})

test('risk: web tools are Network', () => {
  assert.deepEqual(risks({ tool_name: 'WebFetch', tool_input: { url: 'https://a.io' } }), ['network'])
  assert.deepEqual(risks({ tool_name: 'WebSearch', tool_input: { query: 'x' } }), ['network'])
})

test('risk: destructive shell commands are Delete', () => {
  for (const c of ['rm -rf tmp/', 'git clean -fd', 'git reset --hard HEAD~1', 'psql -c "DROP TABLE users"', 'find . -name x -delete']) {
    assert.deepEqual(risks(bash(c)), ['delete'], c)
  }
  assert.deepEqual(risks(bash('git push --force origin main')), ['delete', 'network'])
})

test('risk: network shell commands are Network', () => {
  for (const c of ['curl https://x.dev', 'wget http://x.dev/a', 'git push', 'git pull', 'git fetch', 'npm i', 'pnpm add vue']) {
    assert.deepEqual(risks(bash(c)), ['network'], c)
  }
})

test('risk: read-only commands are Read, unknown commands are Write', () => {
  assert.deepEqual(risks(bash('ls -la && git status')), ['read'])
  assert.deepEqual(risks(bash('cat a | grep b')), ['read'])
  assert.deepEqual(risks(bash('some-unknown-tool --go')), ['write'])
  assert.deepEqual(risks(bash('echo hi > out.txt')), ['write'])
  assert.deepEqual(risks(bash('ls 2>&1 | head')), ['read'])
  assert.deepEqual(risks({ tool_name: 'SomethingNew', tool_input: {} }), ['write'])
})

test('risk: one request can carry several labels', () => {
  assert.deepEqual(risks(bash('pnpm test && git push')), ['write', 'network'])
})

test('risk: Codex shell names and array commands', () => {
  assert.deepEqual(risks({ tool_name: 'shell', tool_input: { command: ['rm', '-rf', 'x'] } }), ['delete'])
  assert.deepEqual(risks({ tool_name: 'local_shell', tool_input: { command: 'curl https://a.io' } }), ['network'])
})

test('risk: MCP tools are Network, plus Read or Write by verb', () => {
  assert.deepEqual(risks({ tool_name: 'mcp__github__create_pull_request', tool_input: {} }), ['write', 'network'])
  assert.deepEqual(risks({ tool_name: 'mcp__github__list_issues', tool_input: {} }), ['read', 'network'])
})

test('split: &&, ;, newline, quotes and heredocs', () => {
  assert.deepEqual(splitShellCommands('a && b; c\nd'), ['a', 'b', 'c', 'd'])
  assert.deepEqual(splitShellCommands('echo "a && b" || c'), ['echo "a && b"', 'c'])
  assert.deepEqual(splitShellCommands('cat <<EOF\nx; y\nEOF\nls'), ['cat <<EOF\nx; y\nEOF', 'ls'])
  assert.deepEqual(splitShellCommands('a \\\n  b'), ['a    b'])
  assert.deepEqual(splitShellCommands('  '), [])
})

test('impact: Edit summary counts lines and lists the file', () => {
  const req = { tool_name: 'Edit', cwd: '/w', tool_input: { file_path: 'src/a.ts', old_string: 'x', new_string: 'a\nb\nc\nd\ne\nf' } }
  const imp = permissionImpact(req)
  assert.deepEqual(imp.summary, { key: 'sumEditOne', params: { n: 1, added: 6, removed: 1 } })
  assert.deepEqual(imp.files, [{ path: 'src/a.ts', added: 6, removed: 1 }])
  assert.equal(imp.cwd, '/w')
  assert.deepEqual(imp.network, [])
  assert.deepEqual(imp.risks, ['write'])
})

test('impact: MultiEdit adds up edits, Write counts content only as added', () => {
  const multi = permissionImpact({
    tool_name: 'MultiEdit',
    tool_input: { file_path: 'f', edits: [{ old_string: 'a', new_string: 'b' }, { old_string: 'c\nd', new_string: 'e' }] },
  })
  assert.deepEqual(multi.files, [{ path: 'f', added: 2, removed: 3 }])
  const write = permissionImpact({ tool_name: 'Write', tool_input: { file_path: 'n.md', content: 'a\nb' } })
  assert.equal(write.summary.key, 'sumWrite')
  assert.deepEqual(write.files, [{ path: 'n.md', added: 2, removed: 0 }])
})

test('impact: Codex apply_patch string is parsed per file', () => {
  const patch = '*** Begin Patch\n*** Update File: a.ts\n-old\n+new\n+more\n*** Add File: b.ts\n+x\n*** End Patch'
  const imp = permissionImpact({ tool_name: 'apply_patch', tool_input: patch })
  assert.deepEqual(imp.files, [{ path: 'a.ts', added: 2, removed: 1 }, { path: 'b.ts', added: 1, removed: 0 }])
  assert.equal(imp.summary.key, 'sumEditMany')
})

test('impact: Bash lists each command with its own risks and the network target', () => {
  const imp = permissionImpact(bash('pnpm test && git push'))
  assert.deepEqual(imp.commands, [
    { cmd: 'pnpm test', risks: ['write'] },
    { cmd: 'git push', risks: ['network'] },
  ])
  assert.equal(imp.summary.key, 'sumRunMany')
  assert.deepEqual(imp.network, [{ host: '', kind: 'git', method: 'push' }])
  assert.equal(permissionImpact(bash('ls')).summary.key, 'sumRunOne')
})

test('impact: curl host is guessed from the URL', () => {
  const imp = permissionImpact(bash('curl -s https://api.github.com/repos/x'))
  assert.deepEqual(imp.network, [{ host: 'api.github.com', method: 'GET' }])
})

test('impact: WebFetch names the domain, WebSearch has none', () => {
  const f = permissionImpact({ tool_name: 'WebFetch', tool_input: { url: 'https://docs.rs/serde/latest' } })
  assert.deepEqual(f.summary, { key: 'sumNet', params: { host: 'docs.rs' } })
  assert.deepEqual(f.network, [{ host: 'docs.rs', method: 'GET' }])
  assert.equal(permissionImpact({ tool_name: 'WebSearch', tool_input: { query: 'q' } }).summary.key, 'sumNetNoHost')
})

test('impact: MCP names server and tool', () => {
  const imp = permissionImpact({ tool_name: 'mcp__github__create_pull_request', tool_input: {} })
  assert.equal(imp.mcpServer, 'github')
  assert.equal(imp.mcpTool, 'create_pull_request')
  assert.equal(imp.summary.key, 'sumMcp')
})

test('diff parts: Edit, Write; none for Bash', () => {
  assert.deepEqual(permissionDiffParts({ tool_name: 'Edit', tool_input: { old_string: 'a', new_string: 'b' } }), [{ before: 'a', after: 'b' }])
  assert.deepEqual(permissionDiffParts({ tool_name: 'Write', tool_input: { content: 'z' } }), [{ before: '', after: 'z' }])
  assert.deepEqual(permissionDiffParts(bash('ls')), [])
})

test('purpose: tool_input.description wins', () => {
  const req = bash('ls', { description: 'List the files' })
  assert.equal(permissionPurpose(req, [{ kind: 'text', text: 'ignored' }]), 'List the files')
})

test('purpose: last agent text before the matching tool call, last paragraph only', () => {
  const req = bash('pnpm test')
  const entries = [
    { kind: 'user', text: 'run it' },
    { kind: 'text', text: 'First.\n\nNow I will run the tests.' },
    { kind: 'thinking', text: '…' },
    { kind: 'tool', id: '1', name: 'Bash', input: { command: 'pnpm test' } },
  ]
  assert.equal(permissionPurpose(req, entries), 'Now I will run the tests.')
})

test('purpose: not found or no text gives null; long text is cut', () => {
  assert.equal(permissionPurpose(bash('ls'), []), null)
  assert.equal(permissionPurpose(bash('ls'), null), null)
  assert.equal(permissionPurpose(bash('ls'), [{ kind: 'user', text: 'hi' }]), null)
  const entries = [{ kind: 'text', text: 'x'.repeat(500) }, { kind: 'tool', name: 'Bash', input: { command: 'ls' } }]
  const out = permissionPurpose(bash('ls'), entries)
  assert.equal(out.length, 240)
  assert.ok(out.endsWith('…'))
})

test('purpose: call not logged yet falls back to trailing agent text', () => {
  assert.equal(permissionPurpose(bash('ls'), [{ kind: 'text', text: 'Checking the folder.' }]), 'Checking the folder.')
})

// ── RT-21 (#4): header = purpose, then tool + target; one impact / risk line ──

test('headline: the purpose first, on one line, cut when long', () => {
  const h = permissionHeadline(bash('npm test'), 'Run the unit tests')
  assert.deepEqual(h, { text: 'Run the unit tests', source: 'purpose', full: true })
  const multi = permissionHeadline(bash('ls'), 'List the files\nthen read them')
  assert.equal(multi.text, 'List the files')
  assert.equal(multi.full, false)
  const long = permissionHeadline(bash('ls'), 'x'.repeat(300))
  assert.ok(long.text.length <= 120 && long.text.endsWith('…'))
  assert.equal(long.full, false)
})

test('headline: no purpose → tool name and its target', () => {
  assert.deepEqual(permissionHeadline({ tool_name: 'Edit', tool_input: { file_path: 'src/a.ts' } }, null),
    { text: 'Edit · src/a.ts', source: 'target', full: false })
  assert.equal(permissionHeadline({ tool_name: 'Write', display_name: 'Write file', tool_input: { file_path: 'b.md' } }, '').text,
    'Write file · b.md')
  assert.equal(permissionHeadline({ tool_name: 'WebFetch', tool_input: { url: 'https://docs.x.io/a' } }, null).text,
    'WebFetch · docs.x.io')
  assert.equal(permissionHeadline({ tool_name: 'mcp__google__send_mail', tool_input: {} }, null).text,
    'send_mail · google')
  assert.equal(permissionHeadline(bash('git push origin main\nls'), null).text, 'Bash · git push origin main')
})

test('headline: nothing to say → null (the SDK title stays)', () => {
  assert.equal(permissionHeadline({ tool_name: 'Task', tool_input: {} }, null), null)
  assert.equal(permissionHeadline({ tool_name: 'Bash', tool_input: {} }, '   '), null)
})

test('risk notes: one per risky kind, none for a read', () => {
  assert.deepEqual(permissionRiskNotes(permissionImpact({ tool_name: 'Read', tool_input: { file_path: 'a' } })), [])
  assert.deepEqual(permissionRiskNotes(permissionImpact(bash('rm -rf build && curl https://api.x.io/y'))), [
    { key: 'riskNoteDelete', params: {} },
    { key: 'riskNoteNetworkHost', params: { host: 'api.x.io' } },
  ])
  assert.deepEqual(permissionRiskNotes(permissionImpact({ tool_name: 'Edit', tool_input: { file_path: 'a.ts', old_string: 'a', new_string: 'b' } })),
    [{ key: 'riskNoteWrite', params: {} }])
  assert.deepEqual(permissionRiskNotes(permissionImpact(bash('git push'))), [{ key: 'riskNoteNetwork', params: {} }])
})

test('Impact opens itself for write, delete and network, not for a read', () => {
  assert.equal(impactOpensItself(['read']), false)
  assert.equal(impactOpensItself([]), false)
  for (const r of ['write', 'delete', 'network']) assert.equal(impactOpensItself(['read', r]), true)
})

/** A credential-broker prompt: the CLI as the tool, its argv after the tool name. */
const broker = (tool, ...argv) => ({
  tool_name: tool,
  tool_input: { command: [tool, ...argv].join(' ') },
  cwd: '/w',
  once_only: true,
  broker: { argv },
})
const GRAPHQL_READ = ['api', 'graphql', '-f', 'query=query{issue(number:1078){title}}', '--jq', '.data', '-c']

test('broker: a GraphQL read is Read + Network on api.github.com', () => {
  const impact = permissionImpact(broker('gh', ...GRAPHQL_READ))
  assert.deepEqual(impact.risks, ['read', 'network'])
  assert.deepEqual(impact.network, [{ host: 'api.github.com' }])
})

test('broker: --hostname names the host of a write', () => {
  const impact = permissionImpact(broker('gh', 'api', '--hostname', 'ghe.corp.example', '-X', 'POST', 'repos/o/r/issues'))
  assert.deepEqual(impact.risks, ['write', 'network'])
  assert.deepEqual(impact.network, [{ host: 'ghe.corp.example' }])
})

test('broker: a forced push deletes and goes to the git remote', () => {
  const impact = permissionImpact(broker('git', 'push', '--force'))
  assert.ok(impact.risks.includes('delete'), impact.risks.join())
  assert.deepEqual(impact.network, [{ host: '', kind: 'git', method: 'push' }])
})

test('broker: the Network row is never empty', () => {
  const samples = [
    broker('gh', ...GRAPHQL_READ),
    broker('gh', 'api', '-X', 'DELETE', 'repos/o/r/issues/comments/5'),
    broker('gh', 'pr', 'merge', '12'),
    broker('glab', 'mr', 'merge', '7'),
    broker('git', 'push', 'origin', 'main'),
    broker('terraform', 'apply'),
    broker('az', 'rest', '--url', 'https://management.azure.com/x'),
    broker('aws', 's3', 'rm', 's3://b/k'),
    broker('aws', 'ec2', 'describe-instances', '--frobnicate'),
  ]
  for (const req of samples) {
    const impact = permissionImpact(req)
    assert.ok(impact.network.length > 0, req.tool_input.command)
    assert.ok(impact.risks.includes('network'), req.tool_input.command)
  }
  assert.equal(permissionImpact(broker('az', 'rest', '--url', 'https://management.azure.com/x')).network[0].host, 'management.azure.com')
})

test('broker: the summary is the action name, not "Use tool"', () => {
  assert.deepEqual(permissionImpact(broker('gh', 'pr', 'merge', '12')).summary, {
    key: 'action.prMergeN',
    params: { kind: 'PR', n: '12' },
  })
  assert.equal(classifyPermissionRisk(broker('aws', 'ec2', 'terminate-instances')).join(), 'delete,network')
})

test('broker: the risk note names the host', () => {
  assert.deepEqual(permissionRiskNotes(permissionImpact(broker('gh', ...GRAPHQL_READ))), [
    { key: 'riskNoteNetworkHost', params: { host: 'api.github.com' } },
  ])
})

test('a gh call without the broker flag keeps the generic Write', () => {
  assert.deepEqual(risks({ tool_name: 'gh', tool_input: { command: 'gh pr list' } }), ['write'])
})
