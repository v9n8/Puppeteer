// RT-06: the backend emits every logged prompt to all windows following the
// run; the window that sent it (pushUser) already shows it and drops the echo.
// Run: node --test src/lib/__tests__/promptEcho.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { register } from 'node:module'

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
const { applyPromptEcho } = await import('../promptEcho.ts')
const { createStreamState } = await import('../streamEvents.ts')

const prompt = (content, turn) => ({ type: 'user', message: { role: 'user', content }, ...(turn !== undefined ? { turn } : {}) })
const state = (entries = []) => ({ ...createStreamState(), entries })
const sent = (text, extra = {}) => ({ kind: 'user', text, echoPending: true, ...extra })

test('another window gets the prompt as a user entry with its turn', () => {
  const s = state([{ kind: 'text', text: 'earlier answer' }])
  applyPromptEcho(s, prompt('next step', 4))
  assert.deepEqual(s.entries.at(-1), { kind: 'user', text: 'next step', turn: 4 })
})

test('the sending window keeps its own entry and takes the turn from the echo', () => {
  const mine = sent('next step')
  const s = state([mine])
  applyPromptEcho(s, prompt('next step', 4))
  assert.equal(s.entries.length, 1)
  assert.equal(s.entries[0].turn, 4)
  assert.equal(s.entries[0].echoPending, undefined)
})

test('two identical prompts in a row: each echo claims one entry, in order', () => {
  const s = state([sent('ok'), { kind: 'text', text: 'a' }, sent('ok')])
  applyPromptEcho(s, prompt('ok', 1))
  applyPromptEcho(s, prompt('ok', 2))
  assert.equal(s.entries.length, 3)
  assert.deepEqual([s.entries[0].turn, s.entries[2].turn], [1, 2])
  // A third echo (sent from another window) is new.
  applyPromptEcho(s, prompt('ok', 3))
  assert.equal(s.entries.length, 4)
})

test('an entry that already has another turn is not claimed', () => {
  const s = state([sent('ok', { turn: 7 })])
  applyPromptEcho(s, prompt('ok', 8))
  assert.equal(s.entries.length, 2)
})

test('a seeded history entry with the same text is never claimed', () => {
  const s = state([{ kind: 'user', text: 'continue' }])
  applyPromptEcho(s, prompt('continue'))
  assert.equal(s.entries.length, 2)
})

test('the sent text is compared after trimming; images ride in block content', () => {
  const img = { media_type: 'image/png', data: 'AAAA' }
  const s = state([sent('  look  ', { images: [img] })])
  applyPromptEcho(s, prompt([{ type: 'text', text: 'look' }, { type: 'image', source: { type: 'base64', media_type: 'image/png', data: 'AAAA' } }], 2))
  assert.equal(s.entries.length, 1)
  assert.equal(s.entries[0].turn, 2)
})

test('any other event goes through applyStreamEvent untouched', () => {
  const s = state([sent('hi')])
  applyPromptEcho(s, { type: 'assistant', message: { role: 'assistant', content: [{ type: 'text', text: 'hello' }] } })
  assert.equal(s.entries.length, 2)
  assert.equal(s.entries[0].echoPending, true)
})
