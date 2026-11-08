// Unit tests for lib/skillDoctor.ts (Skill Doctor report in the skill detail and the Allocation matrix).
// Run: node --test src/lib/__tests__/skillDoctor.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  DOCTOR_RULES,
  countBySeverity,
  doctorBadge,
  groupIssuesBySeverity,
  reportsById,
} from '../skillDoctor.ts'

const issue = (rule, severity, params) => ({ rule, severity, ...(params ? { params } : {}) })
const report = (skillId, issues) => ({
  skill_id: skillId,
  name: skillId,
  checked_at: 0,
  issues,
  stats: { file_bytes: 0, body_chars: 0, description_chars: 0, asset_files: 0 },
})

test('groupIssuesBySeverity: worst first, backend order kept inside a group, empty groups dropped', () => {
  const issues = [
    issue('unknown-frontmatter-key', 'info', { key: 'x' }),
    issue('description-no-trigger', 'warn'),
    issue('missing-name', 'error'),
    issue('body-thin', 'warn', { chars: 10, min: 200 }),
  ]
  const groups = groupIssuesBySeverity(issues)
  assert.deepEqual(groups.map((g) => g.severity), ['error', 'warn', 'info'])
  assert.deepEqual(groups[1].issues.map((i) => i.rule), ['description-no-trigger', 'body-thin'])

  assert.deepEqual(groupIssuesBySeverity([issue('body-thin', 'warn')]).map((g) => g.severity), ['warn'])
  assert.deepEqual(groupIssuesBySeverity([]), [])
})

test('countBySeverity: counts each severity, ignores an unknown one', () => {
  const counts = countBySeverity([
    issue('missing-name', 'error'),
    issue('body-empty', 'error'),
    issue('body-thin', 'warn'),
    issue('future-rule', 'fatal'),
  ])
  assert.deepEqual(counts, { error: 2, warn: 1, info: 0 })
})

test('doctorBadge: the worst of errors and warnings, with its count', () => {
  const badge = doctorBadge(report('a', [
    issue('description-no-trigger', 'warn'),
    issue('missing-description', 'error'),
    issue('body-thin', 'warn'),
    issue('unknown-frontmatter-key', 'info'),
  ]))
  assert.deepEqual(badge, { severity: 'error', count: 1, counts: { error: 1, warn: 2 } })

  assert.deepEqual(doctorBadge(report('b', [issue('body-thin', 'warn')])), {
    severity: 'warn',
    count: 1,
    counts: { error: 0, warn: 1 },
  })
})

test('doctorBadge: none for a clean skill, notes only, or no report', () => {
  assert.equal(doctorBadge(report('a', [])), null)
  assert.equal(doctorBadge(report('a', [issue('unknown-frontmatter-key', 'info'), issue('oversized-asset', 'info')])), null)
  assert.equal(doctorBadge(undefined), null)
})

test('reportsById: keys the reports by skill id', () => {
  const byId = reportsById([report('a', []), report('b', [issue('body-empty', 'error')])])
  assert.deepEqual(Object.keys(byId), ['a', 'b'])
  assert.equal(byId.b.issues[0].rule, 'body-empty')
})

test('every rule has a sentence in en and vi with the params the backend sends', () => {
  // Params per rule, as `skill_doctor.rs` builds them.
  const params = {
    'invalid-id': ['id'],
    'name-id-mismatch': ['name', 'id'],
    'unknown-frontmatter-key': ['key'],
    'description-too-short': ['chars', 'min'],
    'description-too-long': ['chars', 'max'],
    'shadowed-id': ['id', 'engine'],
    'duplicate-name': ['name', 'other_id'],
    'body-thin': ['chars', 'min'],
    'body-too-large': ['kb', 'max_kb'],
    'broken-link': ['target'],
    'link-outside-folder': ['target'],
    'oversized-asset': ['path', 'kb'],
  }
  assert.equal(DOCTOR_RULES.length, 17)
  for (const loc of ['en', 'vi']) {
    const rules = JSON.parse(readFileSync(new URL(`../../i18n/locales/${loc}/skillDoctor.json`, import.meta.url), 'utf8')).rule
    assert.deepEqual(Object.keys(rules).sort(), [...DOCTOR_RULES].sort(), loc)
    for (const [rule, names] of Object.entries(params)) {
      const used = [...rules[rule].matchAll(/\{(\w+)\}/g)].map((m) => m[1])
      for (const name of used) assert.ok(names.includes(name), `${loc} ${rule}: {${name}} is not sent`)
    }
  }
})

test('DOCTOR_RULES matches the Rule enum of skill_doctor.rs', () => {
  const rs = readFileSync(new URL('../../../src-tauri/src/commands/skill_doctor.rs', import.meta.url), 'utf8')
  const body = rs.slice(rs.indexOf('pub enum Rule {'), rs.indexOf('}', rs.indexOf('pub enum Rule {')))
  const variants = [...body.matchAll(/^\s+([A-Z]\w*),/gm)].map((m) => m[1].replace(/[A-Z]/g, (c, i) => (i ? '-' : '') + c.toLowerCase()))
  assert.deepEqual(variants, [...DOCTOR_RULES])
})
