// Unit tests for lib/editStats.ts (diffs and +/− counts of file-editing tool calls;
// the Session's "Changed files" tab, V3-26).
// Run: node --test src/lib/__tests__/editStats.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { register } from 'node:module'

// streamEvents.ts imports siblings without an extension (Vite style); resolve
// those to `.ts` so Node's type stripping can load the module graph.
register(
  'data:text/javascript,' +
    encodeURIComponent(`
      export async function resolve(spec, ctx, next) {
        try { return await next(spec, ctx) } catch (e) {
          if (spec.startsWith('.') && !/\\.[cm]?[jt]s$/.test(spec)) return next(spec + '.ts', ctx)
          throw e
        }
      }`),
)
const { changedFiles, editParts, editStats, mergeLiveEdits } = await import('../editStats.ts')
const { parseStreamLogWindow } = await import('../streamEvents.ts')

const tool = (id, name, input, result) => ({ kind: 'tool', id, name, input, ...(result ? { result } : {}) })
const ok = { content: '', is_error: false }
const user = () => ({ kind: 'user', text: 'go' })

test('editStats counts an Edit and is cached per input', () => {
  const input = { file_path: 'a.ts', old_string: 'one\ntwo', new_string: 'one\n2\n3' }
  assert.deepEqual(editStats('Edit', input), { added: 2, removed: 1 })
  assert.equal(editStats('Edit', input), editStats('Edit', input))
  assert.equal(editParts('Edit', input), editParts('Edit', input))
})

test('Write is all added; MultiEdit adds its edits; unknown tools and bad input give nothing', () => {
  assert.deepEqual(editStats('Write', { file_path: 'a', content: 'x\ny\nz' }), { added: 3, removed: 0 })
  const multi = { file_path: 'a', edits: [{ old_string: 'a', new_string: 'b' }, { old_string: 'c', new_string: 'd\ne' }] }
  assert.deepEqual(editStats('MultiEdit', multi), { added: 3, removed: 2 })
  assert.deepEqual(editStats('Bash', { command: 'ls' }), { added: 0, removed: 0 })
  assert.deepEqual(editStats('Edit', null), { added: 0, removed: 0 })
})

test('changedFiles adds every edit of a file and orders by latest edit', () => {
  const entries = [
    user(),
    tool('1', 'Edit', { file_path: 'a.ts', old_string: 'x', new_string: 'y' }, ok),
    tool('2', 'Write', { file_path: 'b.ts', content: '1\n2' }, ok),
    user(),
    tool('3', 'Edit', { file_path: 'a.ts', old_string: 'y', new_string: 'y\nz' }, ok),
  ]
  const files = changedFiles(entries)
  assert.deepEqual(files.map((f) => f.path), ['a.ts', 'b.ts'])
  assert.deepEqual([files[0].added, files[0].removed, files[0].kind], [2, 1, 'edit'])
  assert.deepEqual([files[1].added, files[1].removed, files[1].kind], [2, 0, 'create'])
  assert.deepEqual(files[0].edits.map((e) => e.turn), [1, 2])
})

test('the total of a file equals the sum of its tool calls (same function as the conversation)', () => {
  const calls = [
    tool('1', 'Edit', { file_path: 'a.ts', old_string: 'a\nb\nc', new_string: 'a\nB\nc\nd' }, ok),
    tool('2', 'MultiEdit', { file_path: 'a.ts', edits: [{ old_string: 'q', new_string: 'r' }, { old_string: '', new_string: 's' }] }, ok),
  ]
  const [file] = changedFiles(calls)
  const sum = calls.reduce((n, c) => n + editStats(c.name, c.input).added, 0)
  assert.equal(file.added, sum)
})

test('a failed call is neither counted nor shown; a running one is', () => {
  const files = changedFiles([
    tool('1', 'Edit', { file_path: 'a.ts', old_string: 'x', new_string: 'y' }, { content: 'no', is_error: true }),
    tool('2', 'Edit', { file_path: 'b.ts', old_string: 'x', new_string: 'y' }),
  ])
  assert.deepEqual(files.map((f) => f.path), ['b.ts'])
})

test('read-only tools are ignored', () => {
  assert.deepEqual(changedFiles([tool('1', 'Read', { file_path: 'a.ts' }, ok), tool('2', 'Grep', { path: 'src' }, ok)]), [])
})

test('a call whose text was too large keeps the file but its count is unknown', () => {
  const files = changedFiles([tool('w', 'Write', { file_path: 'big.txt', content: '' }, ok)], new Set(['w']))
  assert.equal(files.length, 1)
  assert.equal(files[0].unknown, true)
  assert.equal(files[0].edits[0].tooLarge, true)
  assert.deepEqual(files[0].edits[0].parts, [])
})

test('a Codex call spanning two files gives each its own parts', () => {
  const input = {
    changes: [
      { path: 'a.rs', kind: 'add', diff: '1\n2\n3' },
      { path: 'b.rs', kind: 'update', diff: '@@ -1 +1 @@\n-old\n+new' },
    ],
  }
  const files = changedFiles([tool('c', 'Edit', input, ok)])
  const by = Object.fromEntries(files.map((f) => [f.path, f]))
  assert.deepEqual([by['a.rs'].added, by['a.rs'].removed], [3, 0])
  assert.deepEqual([by['b.rs'].added, by['b.rs'].removed], [1, 1])
})

test('mergeLiveEdits replaces a scanned call by its live version and appends new ones', () => {
  const scanned = [user(), tool('1', 'Edit', { file_path: 'a', old_string: 'x', new_string: 'y' })]
  const live = [
    tool('1', 'Edit', { file_path: 'a', old_string: 'x', new_string: 'y' }, ok),
    tool('2', 'Write', { file_path: 'b', content: 'z' }, ok),
    tool('3', 'Read', { file_path: 'c' }, ok),
  ]
  const merged = mergeLiveEdits(scanned, live)
  assert.equal(merged.length, 3)
  assert.equal(merged[1].result, ok)
  assert.equal(merged[2].id, '2')
  assert.equal(mergeLiveEdits(scanned, []), scanned)
})

test('records from get_run_edit_records parse into entries: prompt stubs number the turns, failed calls drop out', () => {
  const prompt = { type: 'user', message: { content: [{ type: 'text', text: '.' }] } }
  const use = (id, n) => ({ type: 'assistant', message: { content: [{ type: 'tool_use', id, name: 'Edit', input: { file_path: 'a', old_string: 'x', new_string: n } }] } })
  const res = (id, bad) => ({ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: id, is_error: bad, content: '' }] } })
  const lines = [prompt, use('t1', 'y'), res('t1', false), prompt, use('t2', 'z'), res('t2', true)].map((l) => JSON.stringify(l))
  const [file, ...rest] = changedFiles(parseStreamLogWindow(lines).entries)
  assert.equal(rest.length, 0)
  assert.deepEqual([file.added, file.removed], [1, 1])
  assert.deepEqual(file.edits.map((e) => e.turn), [1])
})
