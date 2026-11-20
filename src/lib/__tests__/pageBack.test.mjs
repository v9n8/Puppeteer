// RT-08: the prompt navigator in the live pane pages back until the target
// prompt is loaded (live paging from RT-07), then scrolls to it.
// Run: node --test src/lib/__tests__/pageBack.test.mjs
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
const { pageBackTo } = await import('../livePaging.ts')

// A log of `total` prompts with the last `loaded` on screen; each page adds `perPage`.
function fakeLog(total, loaded, perPage) {
  const s = { loaded, calls: 0 }
  return {
    s,
    pager: {
      loadedOffset: () => total - s.loaded,
      hasMore: () => s.loaded < total,
      position: () => s.calls,
      loadEarlier: async () => {
        s.calls++
        s.loaded = Math.min(total, s.loaded + perPage)
      },
    },
  }
}

test('pages back until the first prompt is loaded (the "–/3" → "1/3" jump)', async () => {
  const { s, pager } = fakeLog(3, 0, 1)
  assert.equal(await pageBackTo(0, pager), true)
  assert.equal(pager.loadedOffset(), 0)
  assert.equal(s.calls, 3)
})

test('a prompt already loaded needs no page', async () => {
  const { s, pager } = fakeLog(10, 4, 2)
  assert.equal(await pageBackTo(7, pager), true)
  assert.equal(s.calls, 0)
})

test('stops at the prompt, not at the top of the log', async () => {
  const { s, pager } = fakeLog(20, 2, 3)
  assert.equal(await pageBackTo(12, pager), true)
  assert.equal(s.calls, 2)
})

test('gives up when paging makes no progress or nothing is left', async () => {
  let calls = 0
  const stuck = { loadedOffset: () => 5, hasMore: () => true, position: () => 900, loadEarlier: async () => { calls++ } }
  assert.equal(await pageBackTo(0, stuck), false)
  assert.ok(calls <= 3)
  const empty = { loadedOffset: () => 5, hasMore: () => false, position: () => 0, loadEarlier: async () => { throw new Error('not called') } }
  assert.equal(await pageBackTo(0, empty), false)
})

test('pages with no prompt in them (a long turn) keep paging while the cursor moves', async () => {
  let cursor = 10_000
  let offset = 1
  const pager = {
    loadedOffset: () => offset,
    hasMore: () => cursor > 0,
    position: () => cursor,
    loadEarlier: async () => {
      cursor -= 1000
      if (cursor === 2000) offset = 0 // the prompt shows up only after 8 pages
    },
  }
  assert.equal(await pageBackTo(0, pager), true)
  assert.equal(cursor, 2000)
})
