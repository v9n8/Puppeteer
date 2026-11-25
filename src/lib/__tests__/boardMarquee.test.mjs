// Source checks of the board's box select (BD-02, components/board/useBoardMarquee.ts and
// useBoardMarks.ts): press rules that live in DOM listeners, which no pure function covers.
// Run: node --test src/lib/__tests__/boardMarquee.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

const read = (p) => readFileSync(new URL(p, import.meta.url), 'utf8')
const marquee = read('../../components/board/useBoardMarquee.ts')
const marks = read('../../components/board/useBoardMarks.ts')
const body = (src, head) => {
  const m = src.match(new RegExp(`${head} \\{[\\s\\S]*?\\n  \\}`))
  assert.ok(m, `missing: ${head}`)
  return m[0]
}
const onPointerDown = body(marquee, 'function onPointerDown\\(e: PointerEvent\\)')
const stop = body(marquee, 'function stop\\(\\)')

test('BD-02: a box press is not cancelled, so outside-press menus still close and focus still moves', () => {
  // Cancelling pointerdown would drop the compatibility mousedown that menus and popovers close on.
  assert.doesNotMatch(onPointerDown, /preventDefault/)
  assert.match(onPointerDown, /window\.addEventListener\('selectstart', preventSelect\)/)
  assert.match(stop, /window\.removeEventListener\('selectstart', preventSelect\)/)
  assert.match(marquee, /function preventSelect\(e: Event\) \{\s*e\.preventDefault\(\)\s*\}/)
})

test('BD-02: a press on a card list\'s own scrollbar starts no box and keeps the marks', () => {
  assert.match(
    onPointerDown,
    /const list = target\.closest<HTMLElement>\(LIST\)\s*if \(list && e\.clientX - list\.getBoundingClientRect\(\)\.left >= list\.clientLeft \+ list\.clientWidth\) return/,
  )
  // Checked before the press is taken, so the release cannot clear the marks.
  assert.match(marquee, /const LIST = '\[data-board-cards\]'/)
  assert.ok(onPointerDown.indexOf('closest<HTMLElement>(LIST)') < onPointerDown.indexOf('press = {'))
})

test('BD-02: marks the box writes are cut to the cards on screen, so a restore cannot bring back a gone card', () => {
  assert.match(marks, /setMarks: \(next\) => \{ marks\.value = pruneMarks\(next, options\.visible\.value\.map\(\(r\) => \(\{ label: r\.id \}\)\)\) \}/)
})
