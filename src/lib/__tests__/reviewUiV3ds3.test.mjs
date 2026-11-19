// UI review v3, design round DS-3 (one Quote style, option A · left bar): source-level
// checks of template rules that have no pure function to test, in the style of
// reviewUiV3b.test.mjs.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')
// Every static class string (class="…" and '…' literals) of a file.
const classStrings = (src) => [...src.matchAll(/class="([^"]*)"|'([^'\n]*)'/g)].map((m) => m[1] ?? m[2])

test('UR-48: the excerpt has the left bar and never puts `block` next to `line-clamp`', () => {
  const src = read('../../components/QuoteExcerpt.vue')
  assert.match(src, /border-l-2 border-muted-foreground\/50 pl-2 text-muted-foreground/)
  assert.match(src, /'line-clamp-3'/)
  for (const cls of classStrings(src)) {
    if (/\bline-clamp-\d/.test(cls)) assert.doesNotMatch(cls, /\bblock\b/, `block with line-clamp: ${cls}`)
  }
})

test('UR-48: StreamLog draws sent quotes with SentQuoteCard; hover changes only the border', () => {
  const log = read('../../components/StreamLog.vue')
  assert.match(log, /<SentQuoteCard :card="card" :index="ci" @open="openQuote\(card\)" \/>/)
  assert.doesNotMatch(log, /function quoteSourceLabel/)
  const card = read('../../components/SentQuoteCard.vue')
  assert.doesNotMatch(card, /hover:bg-/)
  assert.match(card, /group flex w-full items-start gap-2 rounded-md border border-border bg-card px-2\.5 py-2 text-left transition-colors/)
  assert.match(card, /cursor-pointer hover:border-primary\/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/)
  assert.match(card, /gone \? 'border-dashed'/)
  assert.match(card, /<QuoteExcerpt[\s\S]*?:lines="3"/)
  assert.match(card, /mt-1 block whitespace-pre-wrap pl-2\.5 text-md leading-\[1\.6\] text-foreground/)
  assert.match(card, /group-hover:opacity-100[^"]*group-focus-visible:opacity-100/)
})
