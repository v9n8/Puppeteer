// Unit tests for what lib/streamEvents.ts keeps for structured quotes (UX-29):
// the id of the message a text entry came from (the anchor of a quote), and
// the quote set a prompt carried.
// Run: node --test src/lib/__tests__/streamEventsQuotes.test.mjs
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
const { applyStreamEvent, createStreamState, parseStreamLogWindow } = await import('../streamEvents.ts')

const assistant = (id, content, extra = {}) => ({
  type: 'assistant',
  message: { ...(id ? { id } : {}), content },
  parent_tool_use_id: null,
  ...extra,
})
const text = (t) => ({ type: 'text', text: t })

function run(events) {
  const state = createStreamState()
  for (const e of events) applyStreamEvent(state, e)
  return state.entries
}

test('a text entry carries its message id and which of its text blocks it is', () => {
  // The SDK sends a message one block per event, all under one id.
  const entries = run([
    assistant('msg_A', [{ type: 'thinking', thinking: 'hmm' }]),
    assistant('msg_A', [text('first')]),
    assistant('msg_A', [text('second')]),
    assistant('msg_A', [{ type: 'tool_use', id: 't1', name: 'Bash', input: {} }]),
    assistant('msg_B', [text('next message'), text('same event')]),
  ])
  const texts = entries.filter((e) => e.kind === 'text').map((e) => [e.text, e.mid, e.block])
  assert.deepEqual(texts, [
    ['first', 'msg_A', 0],
    ['second', 'msg_A', 1],
    ['next message', 'msg_B', 0],
    ['same event', 'msg_B', 1],
  ])
})

test('a log replayed in windows gives the same ids (they come from the record, not the position)', () => {
  const lines = [
    JSON.stringify({ type: 'user', message: { role: 'user', content: 'go' } }),
    JSON.stringify(assistant('msg_A', [text('one')])),
    JSON.stringify(assistant('msg_B', [text('two')])),
  ]
  const whole = parseStreamLogWindow(lines).entries.filter((e) => e.kind === 'text')
  const tail = parseStreamLogWindow(lines.slice(2)).entries.filter((e) => e.kind === 'text')
  assert.deepEqual(tail[0], whole[1])
  assert.deepEqual(whole.map((e) => e.mid), ['msg_A', 'msg_B'])
})

test('no message id (an old Codex log): no anchor, the entry is as before', () => {
  assert.deepEqual(run([assistant(null, [text('hi')])]), [{ kind: 'text', text: 'hi' }])
})

test('a streamed block gets its id when the complete message replaces it', () => {
  const se = (event) => ({ type: 'stream_event', event, parent_tool_use_id: null })
  const entries = run([
    se({ type: 'content_block_start', index: 0, content_block: { type: 'text', text: '' } }),
    se({ type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: 'Hel' } }),
    assistant('msg_A', [text('Hello')]),
  ])
  assert.deepEqual(entries, [{ kind: 'text', text: 'Hello', mid: 'msg_A', block: 0 }])
})

test('a prompt record names its quote set; one without keeps its old shape', () => {
  const entries = run([
    { type: 'user', message: { role: 'user', content: '> cat\n\nwhy?' }, turn: 2, quote_set: 'set-1' },
    { type: 'user', message: { role: 'user', content: 'plain' } },
  ])
  assert.deepEqual(entries, [
    { kind: 'user', text: '> cat\n\nwhy?', turn: 2, quoteSet: 'set-1' },
    { kind: 'user', text: 'plain' },
  ])
})
