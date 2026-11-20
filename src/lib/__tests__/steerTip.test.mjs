// RT-12 (#25): the Steer button explains Steer vs Queue in an InfoTip that opens
// at once on hover (a native `title` shows late). InfoTip gets a `trigger` slot;
// without it, it stays the ⓘ button.
// Run: node --test src/lib/__tests__/steerTip.test.mjs
import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')
const json = (p) => JSON.parse(read(p))

test('InfoTip: a trigger slot wraps the given control, the default is still the ⓘ button', () => {
  const src = read('../../components/ui/InfoTip.vue')
  const template = src.slice(src.indexOf('<template>'))
  assert.match(template, /<slot name="trigger" \/>/)
  // The wrapper opens on hover and keyboard focus inside it, and does not pin on click
  // (the click belongs to the wrapped control).
  const wrap = template.slice(template.indexOf('<span'), template.indexOf('<slot name="trigger"'))
  assert.match(wrap, /v-if="\$slots\.trigger"/)
  assert.match(wrap, /@mouseenter="show"/)
  assert.match(wrap, /@focusin="show"/)
  assert.doesNotMatch(wrap, /@click/)
  // Default ⓘ button is unchanged: hover, focus and click-to-pin.
  const button = template.slice(template.indexOf('<button'), template.indexOf('</button>'))
  assert.match(button, /v-else/)
  assert.match(button, /@click="onClick"/)
  assert.match(button, /<Info class="h-3\.5 w-3\.5"/)
})

test('the Steer button sits in an InfoTip trigger, without a native title', () => {
  const src = read('../../components/sessions/RunComposer.vue')
  const i = src.search(/<InfoTip [^>]*:text="t\('run\.steerTitle'\)"/)
  assert.ok(i > 0, 'InfoTip with run.steerTitle')
  const block = src.slice(i, src.indexOf('</InfoTip>', i))
  assert.match(block, /<template #trigger>/)
  assert.match(block, /handleSendFollowUp\(true\)/)
  assert.doesNotMatch(block, /:title="t\('run\.steerTitle'\)"/)
})

test('run.steerTitle says how Steer differs from Queue (en + vi)', () => {
  const en = json('../../i18n/locales/en/run.json').steerTitle
  const vi = json('../../i18n/locales/vi/run.json').steerTitle
  for (const s of [en, vi]) {
    assert.match(s, /^Steer: /)
    assert.match(s, /Queue \(Enter\): /)
  }
})
