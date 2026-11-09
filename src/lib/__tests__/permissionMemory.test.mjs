// Unit tests for lib/permissionMemory.ts (standing "Always" permission choices).
// Run: node --test src/lib/__tests__/permissionMemory.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { canRememberPermission, standingPermissionDecision } from '../permissionMemory.ts'

const req = (tool_name, once_only) => (once_only === undefined ? { tool_name } : { tool_name, once_only })

test('canRemember: keyed tools yes, plan review and bare MCP no', () => {
  assert.equal(canRememberPermission(req('Bash')), true)
  assert.equal(canRememberPermission(req('mcp__google')), true)
  assert.equal(canRememberPermission(req('ExitPlanMode')), false)
  assert.equal(canRememberPermission(req('MCP')), false)
})

test('canRemember: a once-only broker prompt never offers Always', () => {
  assert.equal(canRememberPermission(req('terraform', true)), false)
  assert.equal(canRememberPermission(req('gh', true)), false)
  assert.equal(canRememberPermission(req('gh', false)), true)
})

test('standing choice: allow / deny lists apply as before', () => {
  assert.equal(standingPermissionDecision(req('Bash'), ['Bash'], []), 'allow')
  assert.equal(standingPermissionDecision(req('Bash'), [], ['Bash']), 'deny')
  assert.equal(standingPermissionDecision(req('Bash'), ['Bash'], ['Bash']), 'deny')
  assert.equal(standingPermissionDecision(req('Bash'), ['Edit'], []), null)
  assert.equal(standingPermissionDecision(req('AskUserQuestion'), ['AskUserQuestion'], []), null)
})

test('standing choice: a once-only prompt is asked even if the tool is on the allow list', () => {
  // An Always saved for `terraform apply` must not auto-allow a later `terraform destroy`.
  assert.equal(standingPermissionDecision(req('terraform', true), ['terraform'], []), null)
  assert.equal(standingPermissionDecision(req('gh', false), ['gh'], []), 'allow')
})

test('standing choice: a once-only prompt is still auto-denied by the deny list', () => {
  assert.equal(standingPermissionDecision(req('aws', true), [], ['aws']), 'deny')
  assert.equal(standingPermissionDecision(req('aws', true), ['aws'], ['aws']), 'deny')
})
