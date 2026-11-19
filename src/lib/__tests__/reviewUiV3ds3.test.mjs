// UI review v3, design round DS-3 (one Quote style): source-level checks of template
// rules that have no pure function to test, in the style of reviewUiV3b.test.mjs.
// UR-58 moved the style from option A (left bar, border hover) to option B (quote
// mark + italic, note under a dashed line with a corner arrow, foreground/5 hover
// fill), so the class pins of UR-48/49 below follow B.
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')
// Every static class string (class="…" and '…' literals) of a file.
const classStrings = (src) => [...src.matchAll(/class="([^"]*)"|'([^'\n]*)'/g)].map((m) => m[1] ?? m[2])

test('UR-58: the excerpt has a quote mark and italic text (upright mono for a file), no left bar, never `block` next to `line-clamp`', () => {
  const src = read('../../components/QuoteExcerpt.vue')
  assert.doesNotMatch(src, /border-l-/)
  assert.match(src, /class="flex min-w-0 gap-1\.5 text-muted-foreground"/)
  assert.match(src, /<Quote class="h-3 w-3 shrink-0" :class="truncate \? '' : 'mt-1'"/)
  assert.match(src, /mono \? 'font-mono' : 'italic'/)
  assert.match(src, /'line-clamp-3'/)
  for (const cls of classStrings(src)) {
    if (/\bline-clamp-\d/.test(cls)) assert.doesNotMatch(cls, /\bblock\b/, `block with line-clamp: ${cls}`)
  }
})

test('UR-48/58: StreamLog draws sent quotes with SentQuoteCard; hover tints the card, the note sits under a dashed line', () => {
  const log = read('../../components/StreamLog.vue')
  assert.match(log, /<SentQuoteCard :card="card" :index="ci" @open="openQuote\(card\)" \/>/)
  assert.doesNotMatch(log, /function quoteSourceLabel/)
  const card = read('../../components/SentQuoteCard.vue')
  assert.doesNotMatch(card, /hover:bg-accent|hover:border-/)
  assert.match(card, /group flex w-full items-start gap-2 rounded-md border border-border bg-card px-2\.5 py-2 text-left transition-colors/)
  assert.match(card, /cursor-pointer hover:bg-foreground\/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/)
  assert.match(card, /gone \? 'border-dashed'/)
  assert.match(card, /<QuoteExcerpt[\s\S]*?:lines="3"/)
  assert.match(card, /class="mt-1\.5 flex gap-1\.5 border-t border-dashed border-border pt-1\.5 text-foreground">\s*<CornerDownRight class="mt-0\.5 h-3\.5 w-3\.5 shrink-0 text-muted-foreground"/)
  assert.match(card, /class="block min-w-0 whitespace-pre-wrap text-md leading-\[1\.6\]/)
  assert.match(card, /group-hover:opacity-100[^"]*group-focus-visible:opacity-100/)
})

test('UR-49/58: composer rows use the Quote style; hover tints the row, a dead row says why', () => {
  const src = read('../../components/sessions/ComposerQuoteBlock.vue')
  assert.match(src, /<QuoteExcerpt :text="q\.excerpt" truncate :mono="q\.kind === 'file'"/)
  assert.doesNotMatch(src, /hover:bg-accent\/60/)
  assert.match(src, /:class="linkOf\(q\) \? 'transition-colors hover:bg-foreground\/5' : ''"/)
  assert.match(src, /class="mt-1 flex gap-1\.5 border-t border-dashed border-border pt-1 text-foreground"[\s\S]*?<CornerDownRight class="mt-0\.5 h-3\.5 w-3\.5 shrink-0 text-muted-foreground"[\s\S]*?class="block min-w-0 truncate text-xs"/)
  assert.doesNotMatch(src, /pl-2\.5 text-xs text-foreground/)
  assert.match(src, /<History v-if="missing\(q\) === 'notLoaded'"[\s\S]*?<Unlink v-else[\s\S]*?t\('run\.quotes\.notLoadedShort'\) : t\('streamLog\.quotes\.sourceGone'\)/)
  for (const [lang, text] of [['en', 'Not loaded'], ['vi', 'Chưa nạp']]) {
    assert.equal(JSON.parse(read(`../../i18n/locales/${lang}/run.json`)).quotes.notLoadedShort, text)
  }
})

test('UR-50: the Quote note popover follows the v3 popover frame and shows the excerpt in the Quote style', () => {
  const src = read('../../components/QuoteNotePopover.vue')
  assert.match(src, /rounded-xl border border-border bg-popover shadow-xl/)
  assert.doesNotMatch(src, /bg-card|bg-muted\/40/)
  assert.match(src, /class="flex h-9 shrink-0 cursor-move select-none items-center gap-1\.5 border-b border-border px-3 text-sm font-medium"\s+:title="t\('selection\.quoteNote\.move'\)"\s+@pointerdown="startMove"/)
  assert.match(src, /<Quote class="h-3\.5 w-3\.5 shrink-0 text-muted-foreground"/)
  assert.match(src, /class="flex min-h-0 flex-1 flex-col gap-2 p-3">\s*<div class="max-h-\[8\.2em\] shrink-0 overflow-y-auto">\s*<QuoteExcerpt :text="text" class="text-md leading-\[1\.6\]" \/>/)
  assert.match(src, /<Textarea[^>]*size="md"/)
  assert.match(src, /class="flex shrink-0 justify-end gap-2 px-3 pb-3">\s*<Button variant="ghost" size="xs"[\s\S]*?<Button size="xs" @click="save">/)
  assert.match(src, /@pointerdown="startResize"/)
})
