// Unit tests for lib/configImport.ts (the Import screen's selection state).
// Run: node --test src/lib/__tests__/configImport.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  buildPicks,
  candidateSections,
  conflictOf,
  countOutcomes,
  importReady,
  importedKinds,
  initialChoices,
  nameError,
  sourceAdapters,
  sourceKey,
  sourceRef,
  visibleSources,
  withAllPicked,
  withChoice,
} from '../configImport.ts'

const cand = (id, kind, name, extra = {}) => ({
  id,
  kind,
  name,
  description: '',
  origin: `/src/${id}`,
  target: kind === 'skill' || kind === 'rule' ? 'claude' : null,
  hash: 'h',
  exists: false,
  mcp: null,
  ...extra,
})

const candidates = [
  cand('mcp:github', 'mcp', 'github'),
  cand('skills:.claude/skills/pdf', 'skill', 'pdf', { target: 'both' }),
  cand('rules:CLAUDE.md', 'rule', 'CLAUDE', { exists: true }),
  cand('rules:.claude/rules/CLAUDE.md', 'rule', 'CLAUDE'),
  cand('agents:.claude/agents/review.md', 'agent', 'review'),
]

const source = (adapter, scope, projectId, kinds) => ({
  adapter,
  scope,
  project_id: projectId,
  project_name: projectId ? projectId.toUpperCase() : null,
  root: '/root',
  kinds,
})

test('visibleSources drops the kinds whose library is off, then sources left empty', () => {
  const sources = [
    source('claude_code', 'global', null, ['agent']),
    source('claude_code', 'project', 'p1', ['rule', 'agent', 'command']),
    source('codex', 'global', null, ['skill']),
  ]
  const on = new Set(['library.commands'])
  const got = visibleSources(sources, (f) => on.has(f))
  assert.deepEqual(
    got.map((s) => [s.adapter, s.scope, s.project_id, s.kinds]),
    [
      ['claude_code', 'project', 'p1', ['rule', 'command']],
      ['codex', 'global', null, ['skill']],
    ],
  )
  // The input is left as it was.
  assert.deepEqual(sources[1].kinds, ['rule', 'agent', 'command'])
})

test('sourceAdapters lists each adapter once, in backend order (a new tool needs no code)', () => {
  const sources = [
    source('claude_code', 'global', null, ['mcp']),
    source('codex', 'global', null, ['skill']),
    source('claude_code', 'project', 'p1', ['rule']),
    source('gemini_cli', 'project', 'p1', ['rule']),
  ]
  assert.deepEqual(sourceAdapters(sources), ['claude_code', 'codex', 'gemini_cli'])
})

test('sourceRef keeps only what the commands take; sourceKey tells sources apart', () => {
  const s = source('claude_code', 'project', 'p1', ['rule'])
  assert.deepEqual(sourceRef(s), { adapter: 'claude_code', scope: 'project', project_id: 'p1' })
  const keys = new Set([
    sourceKey(sourceRef(s)),
    sourceKey(sourceRef(source('claude_code', 'project', 'p2', ['rule']))),
    sourceKey(sourceRef(source('claude_code', 'global', null, ['rule']))),
    sourceKey(sourceRef(source('codex', 'project', 'p1', ['rule']))),
  ])
  assert.equal(keys.size, 4)
})

test('initialChoices ticks new items, leaves taken names unticked, defaults the engine', () => {
  const choices = initialChoices(candidates)
  assert.deepEqual(choices['mcp:github'], { picked: true, name: 'github', target: null, onConflict: 'rename' })
  assert.deepEqual(choices['skills:.claude/skills/pdf'], { picked: true, name: 'pdf', target: 'both', onConflict: 'rename' })
  assert.equal(choices['rules:CLAUDE.md'].picked, false)
  assert.equal(choices['agents:.claude/agents/review.md'].target, null)
  // A skill or rule the adapter gives no engine goes to Claude, as the backend does.
  assert.equal(initialChoices([cand('rules:x', 'rule', 'x', { target: null })])['rules:x'].target, 'claude')
})

test('withChoice and withAllPicked return new objects and ignore unknown ids', () => {
  const choices = initialChoices(candidates)
  const renamed = withChoice(choices, 'mcp:github', { name: 'gh' })
  assert.equal(renamed['mcp:github'].name, 'gh')
  assert.equal(choices['mcp:github'].name, 'github')
  assert.equal(withChoice(choices, 'nope', { picked: false }), choices)

  const none = withAllPicked(choices, candidates.map((c) => c.id).concat('nope'), false)
  assert.ok(Object.values(none).every((c) => !c.picked))
  assert.equal(Object.keys(none).length, candidates.length)
  const some = withAllPicked(none, ['rules:CLAUDE.md'], true)
  assert.deepEqual(
    Object.entries(some).filter(([, c]) => c.picked).map(([id]) => id),
    ['rules:CLAUDE.md'],
  )
})

test('nameError follows the backend library names', () => {
  assert.equal(nameError(''), 'empty')
  assert.equal(nameError('my-skill_2'), null)
  assert.equal(nameError('my skill'), 'invalid')
  assert.equal(nameError('../x'), 'invalid')
  assert.equal(nameError('kỹ-năng'), 'invalid')
})

test('conflictOf: a taken proposed name, then a name an earlier pick of the same kind uses', () => {
  let choices = initialChoices(candidates)
  // Unticked: nothing to resolve.
  assert.equal(conflictOf(candidates, choices, 'rules:CLAUDE.md'), null)
  choices = withChoice(choices, 'rules:CLAUDE.md', { picked: true })
  assert.equal(conflictOf(candidates, choices, 'rules:CLAUDE.md'), 'exists')
  // The second CLAUDE rule comes after a ticked one with the same name.
  assert.equal(conflictOf(candidates, choices, 'rules:.claude/rules/CLAUDE.md'), 'duplicate')
  // Renaming the first one clears both: a new name is not known to be taken.
  choices = withChoice(choices, 'rules:CLAUDE.md', { name: 'project-notes' })
  assert.equal(conflictOf(candidates, choices, 'rules:CLAUDE.md'), null)
  assert.equal(conflictOf(candidates, choices, 'rules:.claude/rules/CLAUDE.md'), null)
  // Same name, other kind: no clash (each library has its own names).
  choices = withChoice(choices, 'agents:.claude/agents/review.md', { name: 'github' })
  assert.equal(conflictOf(candidates, choices, 'agents:.claude/agents/review.md'), null)
  assert.equal(conflictOf(candidates, choices, 'unknown'), null)
})

test('candidateSections groups by kind in screen order, empty kinds left out', () => {
  const sections = candidateSections([...candidates].reverse())
  assert.deepEqual(
    sections.map((s) => [s.kind, s.candidates.map((c) => c.id)]),
    [
      ['mcp', ['mcp:github']],
      ['rule', ['rules:.claude/rules/CLAUDE.md', 'rules:CLAUDE.md']],
      ['skill', ['skills:.claude/skills/pdf']],
      ['agent', ['agents:.claude/agents/review.md']],
    ],
  )
  assert.deepEqual(candidateSections([]), [])
})

test('buildPicks sends the ticked ones in scan order, a name or target only when changed', () => {
  let choices = initialChoices(candidates)
  choices = withChoice(choices, 'mcp:github', { name: 'gh', onConflict: 'skip' })
  choices = withChoice(choices, 'skills:.claude/skills/pdf', { target: 'codex' })
  choices = withChoice(choices, 'rules:.claude/rules/CLAUDE.md', { picked: false })
  // An agent has no engine choice: a stray target is never sent.
  choices = withChoice(choices, 'agents:.claude/agents/review.md', { target: 'codex' })
  assert.deepEqual(buildPicks(candidates, choices), [
    { id: 'mcp:github', on_conflict: 'skip', name: 'gh' },
    { id: 'skills:.claude/skills/pdf', on_conflict: 'rename', target: 'codex' },
    { id: 'agents:.claude/agents/review.md', on_conflict: 'rename' },
  ])
  // Back to the proposed engine: nothing to override.
  choices = withChoice(choices, 'skills:.claude/skills/pdf', { target: 'both' })
  assert.deepEqual(buildPicks(candidates, choices)[1], { id: 'skills:.claude/skills/pdf', on_conflict: 'rename' })
})

test('importReady needs a ticked candidate and valid names on every ticked one', () => {
  let choices = initialChoices(candidates)
  assert.equal(importReady(candidates, choices), true)
  choices = withChoice(choices, 'mcp:github', { name: 'git hub' })
  assert.equal(importReady(candidates, choices), false)
  // An unticked bad name does not block.
  choices = withChoice(choices, 'mcp:github', { picked: false })
  assert.equal(importReady(candidates, choices), true)
  choices = withAllPicked(choices, candidates.map((c) => c.id), false)
  assert.equal(importReady(candidates, choices), false)
  assert.equal(importReady([], {}), false)
})

test('countOutcomes and importedKinds read the import result', () => {
  const outcomes = [
    { id: 'a', kind: 'skill', status: 'imported', name: 'a', message: null },
    { id: 'b', kind: 'mcp', status: 'imported', name: 'b-2', message: null },
    { id: 'c', kind: 'rule', status: 'skipped', name: 'c', message: null },
    { id: 'd', kind: 'agent', status: 'failed', name: null, message: 'boom' },
    { id: 'e', kind: null, status: 'failed', name: null, message: 'not found in the source any more' },
  ]
  assert.deepEqual(countOutcomes(outcomes), { imported: 2, skipped: 1, failed: 2 })
  assert.deepEqual(importedKinds(outcomes), ['mcp', 'skill'])
  assert.deepEqual(importedKinds([]), [])
})
