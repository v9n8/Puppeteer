import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (name) => readFileSync(new URL(`../../components/projects/${name}`, import.meta.url), 'utf8')

test('V3-47 AC-12: conflict card stays one column at a 900px window (card ~568px)', () => {
  const src = read('ProjectConflicts.vue')
  assert.doesNotMatch(src, /@md:/)
  assert.match(src, /@\[40rem\]:grid-cols-/)
})

test('V3-48 AC-10: conflict title button in the health card is at least 28px tall', () => {
  const src = read('ProjectHealthCard.vue')
  const btn = src.match(/<button\s+v-if="row\.[a-z]+ === 'conflicts'[\s\S]*?>/)?.[0] ?? ''
  assert.match(btn, /\bmin-h-7\b/)
})

test('V3-64 AC-13: Model card links and footer button are at least 28px tall', () => {
  const src = read('ProjectModels.vue')
  const links = src.match(/<RouterLink[^>]*>/g) ?? []
  assert.ok(links.length > 0)
  for (const l of links) assert.match(l, /\bmin-h-7\b/)
  const btn = src.match(/<button\s+type="button"\s+class="[^"]*cursor-pointer text-primary[^"]*"/)?.[0] ?? ''
  assert.match(btn, /\bmin-h-7\b/)
})
