// RT-24: the images of a sent prompt open in a viewer (Esc closes, ‹ / › step).
// Run: node --test src/lib/__tests__/promptImages.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { imageDataUrl, stepImageIndex } from '../promptImages.ts'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')
const json = (p) => JSON.parse(read(p))

test('imageDataUrl: a base64 attachment becomes its data URL', () => {
  assert.equal(imageDataUrl({ media_type: 'image/png', data: 'AAAA' }), 'data:image/png;base64,AAAA')
})

test('stepImageIndex: ‹ / › wrap round both ends', () => {
  assert.equal(stepImageIndex(0, 1, 3), 1)
  assert.equal(stepImageIndex(2, 1, 3), 0)
  assert.equal(stepImageIndex(0, -1, 3), 2)
  assert.equal(stepImageIndex(0, -1, 1), 0)
  assert.equal(stepImageIndex(0, 1, 0), 0)
})

test('StreamLog shows the prompt images through PromptImages, which opens a viewer', () => {
  const log = read('../../components/StreamLog.vue')
  assert.match(log, /<PromptImages\s/)
  assert.doesNotMatch(log, /v-for="\(img, ii\) in entry\.images"/)
  const comp = read('../../components/sessions/PromptImages.vue')
  // A thumbnail is a button that opens the viewer; Esc closes it.
  assert.match(comp, /<button[\s\S]*?@click="open\(ii\)"/)
  assert.match(comp, /e\.key === 'Escape'/)
  assert.match(comp, /<Teleport to="body">/)
})

test('the viewer labels exist in en and vi', () => {
  for (const lang of ['en', 'vi']) {
    const v = json(`../../i18n/locales/${lang}/streamLog.json`).promptImages
    for (const k of ['open', 'alt', 'close', 'prev', 'next', 'count']) {
      assert.equal(typeof v?.[k], 'string', `${lang}.streamLog.promptImages.${k}`)
    }
  }
})
