// Unit tests for lib/turnActivity.ts (folding a turn's tool calls, thinking and
// commentary into one block). Run: node --test src/lib/__tests__/turnActivity.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { activityMatches, activityOpen, foldsAway, formatActivityDuration, turnActivity } from '../turnActivity.ts'

const user = (text) => ({ kind: 'user', text })
const text = (t, extra = {}) => ({ kind: 'text', text: t, ...extra })
const thinking = (t) => ({ kind: 'thinking', text: t })
const tool = (name, input = {}, result) => ({ kind: 'tool', id: name, name, input, ...(result ? { result } : {}) })
const ok = (content = 'ok') => ({ content, is_error: false })
const fail = (content = 'boom') => ({ content, is_error: true })
const result = (extra = {}) => ({ kind: 'result', text: '', is_error: false, ...extra })

// [header at, keyAt, members, steps, filesEdited, errors, durationMs, running] per block.
const blocks = (entries, running = false) =>
  turnActivity(entries, running).blocks.map((b) => [
    b.at, b.keyAt, b.members, b.steps, b.filesEdited, b.errors, b.durationMs, b.running,
  ])

test('turnActivity: tools, thinking and commentary fold; the final answer stays out', () => {
  const entries = [
    user('fix it'),
    thinking('hm'),
    text('Looking at the file.'),
    tool('Read', { file_path: '/a.ts' }, ok()),
    tool('Edit', { file_path: '/a.ts', old_string: 'a', new_string: 'b' }, ok()),
    text('Fixed: renamed a to b.'),
    result({ duration_ms: 12300 }),
  ]
  assert.deepEqual(blocks(entries), [[1, 0, [1, 2, 3, 4], 3, 1, 0, 12300, false]])
  const map = turnActivity(entries, false)
  assert.equal(map.headerAt.get(1), 0)
  assert.equal(map.memberOf.has(5), false, 'the answer is not folded')
  assert.equal(map.memberOf.has(6), false, 'the result is not folded')
})

test('turnActivity: several assistant texts — only those after the last step are the answer', () => {
  const entries = [user('q'), text('first'), tool('Bash', { command: 'ls' }, ok()), text('second'), tool('Grep', {}, ok()), text('answer 1'), text('answer 2')]
  const map = turnActivity(entries, false)
  assert.deepEqual(map.blocks[0].members, [1, 2, 3, 4])
  assert.equal(map.memberOf.has(5), false)
  assert.equal(map.memberOf.has(6), false)
})

test('turnActivity: a turn of prose only gets no block', () => {
  assert.deepEqual(blocks([user('hi'), text('hello'), result()]), [])
})

test('turnActivity: a turn ending on a tool folds everything up to it (no answer)', () => {
  const entries = [user('q'), text('let me run it'), tool('Bash', { command: 'make' })]
  assert.deepEqual(blocks(entries), [[1, 0, [1, 2], 1, 0, 0, null, false]])
})

test('turnActivity: each turn has its own block, keyed by its prompt', () => {
  const entries = [user('a'), tool('Read', {}, ok()), text('A'), user('b'), tool('Glob', {}, ok()), text('B')]
  assert.deepEqual(blocks(entries), [
    [1, 0, [1], 1, 0, 0, null, false],
    [4, 3, [4], 1, 0, 0, null, false],
  ])
})

test('turnActivity: entries before the first prompt (window head) form a block keyed by its first member', () => {
  const entries = [text('tail commentary'), tool('Read', {}, ok()), text('tail answer'), user('next')]
  assert.deepEqual(blocks(entries), [[0, 0, [0, 1], 1, 0, 0, null, false]])
})

test('turnActivity: streaming — the last turn runs; its trailing text is the answer until a step follows', () => {
  const live = [user('q'), tool('Read', {}, ok()), text('now I will', { streaming: true })]
  assert.deepEqual(blocks(live, true), [[1, 0, [1], 1, 0, 0, null, true]])
  assert.equal(turnActivity(live, true).memberOf.has(2), false)

  const next = [...live.slice(0, 2), text('now I will'), tool('Bash', { command: 'ls' })]
  assert.deepEqual(turnActivity(next, true).blocks[0].members, [1, 2, 3])

  // Only the last turn is the running one.
  const two = [user('a'), tool('Read', {}, ok()), text('A'), user('b'), tool('Bash', {})]
  assert.deepEqual(turnActivity(two, true).blocks.map((b) => b.running), [false, true])
})

test('turnActivity: a running turn has no duration even when an old result is around', () => {
  const entries = [user('q'), tool('Read', {}, ok()), text('a'), result({ duration_ms: 5000 })]
  assert.equal(turnActivity(entries, true).blocks[0].durationMs, null)
  assert.equal(turnActivity(entries, false).blocks[0].durationMs, 5000)
})

test('turnActivity: errors stay visible; failed tool calls are counted, not shown', () => {
  const entries = [
    user('q'),
    tool('Bash', { command: 'false' }, fail()),
    { kind: 'error', text: 'stderr line' },
    tool('Bash', { command: 'true' }, ok()),
    text('done'),
    result({ is_error: true, text: 'Failed' }),
  ]
  const map = turnActivity(entries, false)
  assert.deepEqual(map.blocks[0].members, [1, 3])
  assert.equal(map.blocks[0].errors, 1)
  assert.equal(map.memberOf.has(2), false, 'a stderr line is never folded')
  assert.equal(map.memberOf.has(5), false, 'a failed result is never folded')
})

test('turnActivity: a question, a plan, a compaction, a slash command and a conductor nudge stay visible', () => {
  const entries = [
    user('q'),
    tool('Read', {}, ok()),
    tool('AskUserQuestion', { questions: [] }, ok('yes')),
    { kind: 'compact', summary: 's' },
    { kind: 'command', name: '/compact' },
    { kind: 'injected', source: 'session', text: 'nudge' },
    { kind: 'injected', source: 'skill', label: 'pdf', text: 'body' },
    { kind: 'log', level: 'info', text: 'note' },
    tool('ExitPlanMode', { plan: '# plan' }),
    text('answer'),
  ]
  const map = turnActivity(entries, false)
  assert.deepEqual(map.blocks[0].members, [1, 6, 7])
  assert.equal(map.blocks[0].steps, 1, 'shown tools are not counted as folded steps')
})

test('turnActivity: a turn whose only steps must stay visible gets no block', () => {
  const entries = [user('q'), text('I need to ask'), tool('AskUserQuestion', {}), text('thanks')]
  assert.deepEqual(blocks(entries), [])
})

test('turnActivity: compact / command / injected do not split a turn', () => {
  const entries = [user('q'), tool('Read', {}, ok()), { kind: 'compact', summary: 's' }, { kind: 'command', name: '/x' }, tool('Grep', {}, ok()), text('a')]
  assert.deepEqual(blocks(entries), [[1, 0, [1, 4], 2, 0, 0, null, false]])
})

test('turnActivity: a steer (a new user message) starts the next turn', () => {
  const entries = [user('q'), tool('Read', {}, ok()), user('steer: also do X'), tool('Edit', { file_path: '/x' }, ok()), text('done')]
  assert.deepEqual(blocks(entries, true), [
    [1, 0, [1], 1, 0, 0, null, false],
    [3, 2, [3], 1, 1, 0, null, true],
  ])
})

test('turnActivity: files edited are distinct, successful edit / write targets (Codex multi-file too)', () => {
  const entries = [
    user('q'),
    tool('Edit', { file_path: '/a.ts' }, ok()),
    tool('Edit', { file_path: '/a.ts' }, ok()),
    tool('Write', { file_path: '/b.ts', content: 'x' }, ok()),
    tool('Edit', { file_path: '/c.ts' }, fail()),
    tool('Read', { file_path: '/d.ts' }, ok()),
    tool('Edit', { changes: [{ path: '/e.ts', kind: 'update', diff: '' }, { path: '/f.ts', kind: 'add', diff: '' }] }, ok()),
    text('done'),
  ]
  assert.equal(turnActivity(entries, false).blocks[0].filesEdited, 4)
})

test('foldsAway: kinds that fold vs. stay', () => {
  assert.equal(foldsAway({ kind: 'tool', name: 'Bash' }), true)
  assert.equal(foldsAway({ kind: 'tool', name: 'mcp__srv__do' }), true)
  assert.equal(foldsAway({ kind: 'tool', name: 'AskUserQuestion' }), false)
  assert.equal(foldsAway({ kind: 'tool', name: 'ExitPlanMode' }), false)
  assert.equal(foldsAway({ kind: 'thinking' }), true)
  for (const kind of ['user', 'result', 'error', 'compact', 'command', 'system', 'ide-context']) {
    assert.equal(foldsAway({ kind }), false, kind)
  }
  assert.equal(foldsAway({ kind: 'injected', source: 'session' }), false)
  assert.equal(foldsAway({ kind: 'injected', source: 'system' }), true)
})

test('activityOpen: a reader choice wins, then a find hit, the running turn, expand-all', () => {
  const done = { running: false }
  const live = { running: true }
  assert.equal(activityOpen(done, {}), false)
  assert.equal(activityOpen(live, {}), true)
  assert.equal(activityOpen(done, { expandAll: true }), true)
  assert.equal(activityOpen(done, { findHit: true }), true)
  assert.equal(activityOpen(live, { choice: false }), false)
  assert.equal(activityOpen(done, { choice: false, findHit: true, expandAll: true }), false)
  assert.equal(activityOpen(done, { choice: true }), true)
})

test('activityMatches: prose, thinking and tool header fields; case handling', () => {
  const entries = [
    user('needle in prompt'),
    text('Checking the Parser'),
    tool('Bash', { command: 'grep -r foo', description: 'Search for foo' }, ok('hidden output needle')),
    thinking('pondering widgets'),
    tool('Read', { file_path: '/src/lib/zebra.ts' }, ok()),
    text('answer mentions needle'),
  ]
  const block = turnActivity(entries, false).blocks[0]
  assert.equal(activityMatches(entries, block, 'parser'), true)
  assert.equal(activityMatches(entries, block, 'parser', true), false)
  assert.equal(activityMatches(entries, block, 'Search for'), true)
  assert.equal(activityMatches(entries, block, 'widgets'), true)
  assert.equal(activityMatches(entries, block, 'zebra'), true)
  assert.equal(activityMatches(entries, block, 'bash'), true)
  // Not in the block: the prompt and the answer are outside; collapsed tool output is not shown.
  assert.equal(activityMatches(entries, block, 'needle'), false)
  assert.equal(activityMatches(entries, block, ''), false)
})

test('formatActivityDuration', () => {
  assert.equal(formatActivityDuration(850), '850ms')
  assert.equal(formatActivityDuration(12_340), '12.3s')
  assert.equal(formatActivityDuration(245_000), '4m 05s')
  assert.equal(formatActivityDuration(3_720_000), '1h 02m')
})
