import test from 'node:test'
import assert from 'node:assert/strict'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'

// Tailwind v4 compiles `max-[900px]:` to `width < 900px`, which never matches
// the 900px minimum window, so the narrow layout must start at 901px.
function* sources(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    if (statSync(p).isDirectory()) {
      if (name !== '__tests__') yield* sources(p)
    } else if (/\.(vue|ts|css)$/.test(name)) yield p
  }
}

test('no max-[900px] / min-[900px] variants in src', () => {
  const bad = []
  for (const f of sources(new URL('../..', import.meta.url).pathname)) {
    if (/(max|min)-\[900px\]/.test(readFileSync(f, 'utf8'))) bad.push(f)
  }
  assert.deepEqual(bad, [])
})
