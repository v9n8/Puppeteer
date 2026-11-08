// Unit tests for lib/libraryItems.ts (Agents / Commands rows, project links, frontmatter).
// Run: node --test src/lib/__tests__/libraryItems.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { GROUP_NONE } from '../listGroups.ts'
import {
  AGENT_TEMPLATE,
  COMMAND_TEMPLATE,
  buildItemRows,
  itemDescription,
  itemName,
  projectLinks,
  setItemField,
  templateOf,
  validateItem,
} from '../libraryItems.ts'

const items = [
  { id: 'a', name: 'code-reviewer', description: 'Reviews diffs' },
  { id: 'b', name: 'debugger', description: 'Finds root causes' },
  { id: 'c', name: 'writer', description: 'Docs and release notes' },
]
const applied = {
  p1: [
    { item_id: 'a', state: 'installed' },
    { item_id: 'b', state: 'drifted' },
  ],
  p2: [{ item_id: 'a', state: 'drifted' }],
  p3: undefined,
}

function rows(over = {}) {
  return buildItemRows(items, { search: '', applied, label: 'All', ...over })
}

test('buildItemRows: one folder, project and drift counts per item', () => {
  const out = rows()
  assert.equal(out.length, 1)
  assert.equal(out[0].key, GROUP_NONE)
  assert.equal(out[0].label, 'All')
  assert.deepEqual(
    out[0].rows.map((r) => [r.id, r.projects, r.drifted]),
    [['a', 2, 1], ['b', 1, 1], ['c', 0, 0]],
  )
  assert.equal(out[0].rows[0].item, items[0])
})

test('buildItemRows: search reaches name and description; no match, no folder', () => {
  assert.deepEqual(rows({ search: 'ROOT' })[0].rows.map((r) => r.id), ['b'])
  assert.deepEqual(rows({ search: 'release writer' })[0].rows.map((r) => r.id), ['c'])
  assert.deepEqual(rows({ search: 'nothing' }), [])
  assert.deepEqual(buildItemRows([], { search: '', applied: {}, label: 'All' }), [])
})

test('projectLinks: every project in order, with the item state or null', () => {
  const projects = [{ id: 'p2' }, { id: 'p1' }, { id: 'p3' }, { id: 'p4' }]
  assert.deepEqual(
    projectLinks(projects, applied, 'a').map((l) => [l.project.id, l.state]),
    [['p2', 'drifted'], ['p1', 'installed'], ['p3', null], ['p4', null]],
  )
  assert.deepEqual(projectLinks(projects, applied, 'c').map((l) => l.state), [null, null, null, null])
})

test('an agent is named by its frontmatter, a command by its own field', () => {
  const agent = '---\nname: "reviewer"\ndescription: Reviews\n---\n\nBody'
  assert.equal(itemName('agent', agent, 'ignored'), 'reviewer')
  assert.equal(itemName('command', agent, '  deploy '), 'deploy')
  assert.equal(itemDescription(agent), 'Reviews')
  assert.equal(itemDescription('no frontmatter'), '')
})

test('validateItem mirrors the backend rules', () => {
  const ok = '---\nname: reviewer\ndescription: d\n---\n'
  assert.equal(validateItem('agent', ok, ''), null)
  assert.equal(validateItem('agent', '---\ndescription: d\n---\n', 'reviewer'), 'errMissingName')
  assert.equal(validateItem('agent', '---\nname: a b\ndescription: d\n---\n', ''), 'errNameSlug')
  assert.equal(validateItem('agent', '---\nname: ../x\ndescription: d\n---\n', ''), 'errNameSlug')
  assert.equal(validateItem('agent', '---\nname: r\n---\n', ''), 'errMissingDescription')
  // Rust's is_alphanumeric takes any script.
  assert.equal(validateItem('agent', '---\nname: kiểm-tra_2\ndescription: d\n---\n', ''), null)

  const cmd = '---\ndescription: Ship it\n---\n\nDeploy $ARGUMENTS'
  assert.equal(validateItem('command', cmd, 'deploy'), null)
  assert.equal(validateItem('command', cmd, '  '), 'errMissingName')
  assert.equal(validateItem('command', cmd, 'de ploy'), 'errNameSlug')
  assert.equal(validateItem('command', 'Deploy $ARGUMENTS', 'deploy'), 'errMissingDescription')
})

test('the templates pass validation', () => {
  assert.equal(templateOf('agent'), AGENT_TEMPLATE)
  assert.equal(templateOf('command'), COMMAND_TEMPLATE)
  assert.equal(validateItem('agent', AGENT_TEMPLATE, ''), null)
  assert.equal(validateItem('command', COMMAND_TEMPLATE, 'my-command'), null)
})

test('setItemField rewrites the frontmatter, or adds one when there is none', () => {
  const raw = '---\nname: r\ndescription: d\n---\n\nBody\n'
  assert.equal(setItemField(raw, 'description', 'x'), '---\nname: r\ndescription: x\n---\n\nBody\n')
  // Cleared, the line goes: an empty `name:` would parse as the next line.
  const cleared = setItemField(raw, 'name', '')
  assert.equal(cleared, '---\ndescription: d\n---\n\nBody\n')
  assert.equal(itemName('agent', cleared, ''), '')
  assert.equal(setItemField(cleared, 'name', 'r2'), '---\ndescription: d\nname: r2\n---\n\nBody\n')
  assert.equal(setItemField('Body\n', 'description', 'x'), '---\ndescription: x\n---\n\nBody\n')
  assert.equal(setItemField('', 'description', 'x'), '---\ndescription: x\n---\n')
  assert.equal(setItemField('Body\n', 'description', ''), 'Body\n')
  assert.equal(setItemField('Body\n', 'description', null), 'Body\n')
})
