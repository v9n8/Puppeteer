// RT-21 (#4): the permission window's header wiring (the rules are in permissionImpact.test.mjs).
// Run: node --test src/lib/__tests__/permissionHeader.test.mjs
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')

test('header: headline as title, SDK title on line 2, one impact / risk line', () => {
  const src = read('../../components/PermissionPrompt.vue')
  assert.match(src, /if \(headline\.value\) return headline\.value\.text/)
  const t = src.slice(src.indexOf('<template>'))
  assert.ok(t.indexOf('{{ headerTitle }}</h2>') < t.indexOf('v-if="sdkTitle"'))
  assert.match(t, /<PermissionRiskBadges :risks="impact\.risks" \/>\s*<span[^>]*>\{\{ impactLine \}\}<\/span>/)
  assert.match(t, /<PermissionRequestBody :request="request" :purpose="bodyPurpose" :expanded="impactOpen" \/>/)
})

test('Impact block follows `expanded` when the request changes', () => {
  assert.match(read('../../components/permission/PermissionImpact.vue'), /watch\(\(\) => \[props\.request, props\.expanded\] as const/)
})
