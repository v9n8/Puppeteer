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
