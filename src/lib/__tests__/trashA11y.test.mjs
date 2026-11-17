import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (name) => readFileSync(new URL(`../../components/trash/${name}`, import.meta.url), 'utf8')

test('Trash group card: project-name button and worker badge are >= 28px tall', () => {
  const src = read('TrashGroupCard.vue')
  const name = src.match(/<button[^>]*?class="[^"]*truncate text-sm font-semibold[^"]*"/s)?.[0]
  assert.match(name, /min-h-7/)
  const workers = src.match(/<button[^>]*?:title="t\('trash\.workersTitle'\)"/s)?.[0]
  assert.match(workers, /min-h-7/)
})

test('Trash toolbar: Search button label follows its text and is 32px wide when icon-only', () => {
  const src = read('TrashToolbar.vue')
  assert.match(src, /:aria-label="searching \? t\('trash\.search\.searching'\) : t\('trash\.search\.submit'\)"/)
  assert.match(src, /max-\[901px\]:w-8/)
  assert.match(src, /max-\[901px\]:px-0/)
})
