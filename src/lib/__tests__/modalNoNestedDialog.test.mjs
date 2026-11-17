// A <Modal> already renders role="dialog" aria-modal="true"; a second dialog
// role inside it makes screen readers announce two nested boxes (V3-R02 regression, V3-47).
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'

const SRC = new URL('../../', import.meta.url).pathname
const MODAL = join(SRC, 'components/ui/Modal.vue')

function* vueFiles(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name)
    if (statSync(p).isDirectory()) yield* vueFiles(p)
    else if (p.endsWith('.vue')) yield p
  }
}

test('no component puts role="dialog" inside a <Modal>', () => {
  const offenders = []
  for (const file of vueFiles(SRC)) {
    if (file === MODAL) continue
    const m = readFileSync(file, 'utf8').match(/<Modal[\s>][\s\S]*<\/Modal>/)
    if (m && /role="dialog"/.test(m[0])) offenders.push(relative(SRC, file))
  }
  assert.deepEqual(offenders, [])
})
