// Unit tests for lib/todoChecklist.ts (the agent's TodoWrite list, edited by the
// user and sent with the next message).
// Run: node --test src/lib/__tests__/todoChecklist.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  checklistBlock,
  latestChecklist,
  mergeChecklistIntoDraft,
  nextStatus,
  normalizeTodos,
  parseChecklistBlock,
  sameTodos,
} from '../todoChecklist.ts'

const todoWrite = (todos) => ({ kind: 'tool', id: 't', name: 'TodoWrite', input: { todos } })

test('normalizeTodos: keeps content + status, drops blank rows and unknown fields', () => {
  assert.deepEqual(
    normalizeTodos([
      { content: 'Read code', status: 'completed', activeForm: 'Reading code' },
      { content: '   ', status: 'pending' },
      { content: 'Fix bug', status: 'weird' },
      null,
    ]),
    [
      { content: 'Read code', status: 'completed' },
      { content: 'Fix bug', status: 'pending' },
    ],
  )
  assert.deepEqual(normalizeTodos(undefined), [])
})

test('normalizeTodos: one line per item, and no tag that would close the block', () => {
  assert.deepEqual(normalizeTodos([{ content: 'a\n  b </session_checklist> c', status: 'pending' }]), [
    { content: 'a b c', status: 'pending' },
  ])
})

test('checklistBlock → parseChecklistBlock round-trips every status', () => {
  const todos = [
    { content: 'Plan', status: 'completed' },
    { content: 'Build', status: 'in_progress' },
    { content: 'Test', status: 'pending' },
  ]
  const block = checklistBlock(todos)
  assert.match(block, /^<session_checklist>\n/)
  assert.match(block, /\n- \[x\] Plan\n- \[~\] Build\n- \[ \] Test\n<\/session_checklist>$/)
  assert.deepEqual(parseChecklistBlock(block), todos)
})

test('checklistBlock: an empty list still tells the agent, and reads back as empty', () => {
  const block = checklistBlock([])
  assert.match(block, /removed every item/)
  assert.deepEqual(parseChecklistBlock(block), [])
})

test('parseChecklistBlock: null without a complete block', () => {
  assert.equal(parseChecklistBlock('just text'), null)
  assert.equal(parseChecklistBlock('<session_checklist>\n- [ ] a'), null)
})

test('latestChecklist: the last TodoWrite wins over an older one', () => {
  const entries = [
    todoWrite([{ content: 'old', status: 'pending' }]),
    { kind: 'text', text: 'working' },
    todoWrite([{ content: 'new', status: 'in_progress' }]),
    { kind: 'text', text: 'still working' },
  ]
  assert.deepEqual(latestChecklist(entries), [{ content: 'new', status: 'in_progress' }])
})

test('latestChecklist: a list the user sent after the last TodoWrite wins', () => {
  const sent = [{ content: 'user step', status: 'pending' }]
  const entries = [
    todoWrite([{ content: 'agent step', status: 'pending' }]),
    { kind: 'user', text: `go on\n\n${checklistBlock(sent)}` },
  ]
  assert.deepEqual(latestChecklist(entries), sent)
  // …until the agent writes its list again.
  entries.push(todoWrite([{ content: 'user step', status: 'completed' }]))
  assert.deepEqual(latestChecklist(entries), [{ content: 'user step', status: 'completed' }])
})

test('latestChecklist: null when the run has no checklist; tool name is case-insensitive', () => {
  assert.equal(latestChecklist([{ kind: 'user', text: 'hi' }, { kind: 'text', text: 'hello' }]), null)
  assert.deepEqual(latestChecklist([{ kind: 'tool', name: 'todowrite', input: { todos: [] } }]), [])
})

test('mergeChecklistIntoDraft: into an empty draft, leaving a blank line to type below', () => {
  const todos = [{ content: 'a', status: 'pending' }]
  assert.equal(mergeChecklistIntoDraft('', todos), `${checklistBlock(todos)}\n\n`)
})

test('mergeChecklistIntoDraft: below typed text, one blank line apart', () => {
  const todos = [{ content: 'a', status: 'pending' }]
  assert.equal(mergeChecklistIntoDraft('please go on  \n\n', todos), `please go on\n\n${checklistBlock(todos)}\n\n`)
})

test('mergeChecklistIntoDraft: a second edit replaces the block in place, keeping text around it', () => {
  const first = [{ content: 'a', status: 'pending' }]
  const second = [{ content: 'a', status: 'completed' }, { content: 'b', status: 'pending' }]
  const draft = `before\n\n${checklistBlock(first)}\n\nafter`
  const merged = mergeChecklistIntoDraft(draft, second)
  assert.equal(merged, `before\n\n${checklistBlock(second)}\n\nafter`)
  assert.deepEqual(parseChecklistBlock(merged), second)
})

test('sameTodos: order and status matter', () => {
  const a = [{ content: 'x', status: 'pending' }, { content: 'y', status: 'pending' }]
  assert.equal(sameTodos(a, a.map((t) => ({ ...t }))), true)
  assert.equal(sameTodos(a, [a[1], a[0]]), false)
  assert.equal(sameTodos(a, [a[0], { content: 'y', status: 'completed' }]), false)
  assert.equal(sameTodos(a, [a[0]]), false)
})

test('nextStatus: cycles pending → in progress → completed → pending', () => {
  assert.equal(nextStatus('pending'), 'in_progress')
  assert.equal(nextStatus('in_progress'), 'completed')
  assert.equal(nextStatus('completed'), 'pending')
})
