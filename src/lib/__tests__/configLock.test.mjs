// Unit tests for lib/configLock.ts (a child switch under an app-wide / project parent).
// Run: node --test src/lib/__tests__/configLock.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { isSlackServer, parentLock, switchView } from '../configLock.ts'

test('a child shows its stored value while no parent locks it', () => {
  assert.deepEqual(switchView(true, null), { checked: true, disabled: false, lockedBy: null })
  assert.deepEqual(switchView(false, null), { checked: false, disabled: false, lockedBy: null })
})

test('a locked child shows off and cannot be switched, whatever it stores', () => {
  assert.deepEqual(switchView(true, 'app'), { checked: false, disabled: true, lockedBy: 'app' })
  assert.deepEqual(switchView(false, 'project'), { checked: false, disabled: true, lockedBy: 'project' })
})

test('the stored value comes back once the parent is on again', () => {
  const stored = true
  assert.equal(switchView(stored, parentLock(false)).checked, false)
  assert.equal(switchView(stored, parentLock(true)).checked, true)
})

test('app-wide wins over the project when both are off', () => {
  assert.equal(parentLock(false, false), 'app')
  assert.equal(parentLock(true, false), 'project')
  assert.equal(parentLock(true, true), null)
  assert.equal(parentLock(true), null)
})

test('a Slack MCP server is one whose env keys hold the Slack token', () => {
  assert.equal(isSlackServer({ env_keys: ['FOO', 'SLACK_MCP_XOXP_TOKEN'] }), true)
  assert.equal(isSlackServer({ env_keys: ['SLACK_BOT_TOKEN'] }), false)
  assert.equal(isSlackServer({ env_keys: [] }), false)
})
