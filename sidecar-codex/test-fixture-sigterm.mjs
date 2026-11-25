#!/usr/bin/env node
// Fixture for test-exit-signal.mjs: an "app-server" that gets killed by SIGTERM
// shortly after start (like a crash / OOM-kill), without answering anything.
// A session reads codex's configured MCP servers first (mcp-guard.mjs): none.
if (process.argv[2] === 'mcp') {
  process.stdout.write('[]\n')
  process.exit(0)
}
process.stdin.resume()
setTimeout(() => process.kill(process.pid, 'SIGTERM'), 300)
