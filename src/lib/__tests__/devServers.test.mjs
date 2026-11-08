// Unit tests for lib/devServers.ts (dev servers card of the Processes screen).
// Run: node --test src/lib/__tests__/devServers.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { canStart, canStop, pickProjectId } from '../devServers.ts'

const server = (status) => ({
  name: 'web',
  command: 'pnpm',
  args: ['dev'],
  cwd: '.',
  port: 3000,
  description: null,
  launch: 'cd /p && pnpm dev > /p/.puppeteer/dev-servers/web.log 2>&1',
  log: '/p/.puppeteer/dev-servers/web.log',
  pids: status === 'running' ? [42] : [],
  port_busy: status === 'port_busy',
  status,
})

test('Start only while nothing runs it or holds its port', () => {
  assert.equal(canStart(server('stopped')), true)
  assert.equal(canStart(server('unknown')), true)
  assert.equal(canStart(server('running')), false)
  assert.equal(canStart(server('port_busy')), false)
})

test('Stop only what runs from its declared command', () => {
  assert.equal(canStop(server('running')), true)
  assert.equal(canStop(server('port_busy')), false, 'a process it did not start is left alone')
  assert.equal(canStop(server('stopped')), false)
})

test('the chosen project is kept while it exists', () => {
  assert.equal(pickProjectId(['a', 'b'], 'b'), 'b')
  assert.equal(pickProjectId(['a', 'b'], 'gone'), 'a')
  assert.equal(pickProjectId([], 'gone'), '')
})
