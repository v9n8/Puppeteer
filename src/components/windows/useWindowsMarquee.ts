// Marking cards of the "Open windows" overlay by dragging a box over the grid (DS-5b). The
// press must land on empty space: a press on a card drags it to the drop strip
// (useWindowsDrag). The cards the box touches are marked as it moves (lib/windowsSelection);
// ⇧ / ⌘ held at the press adds them to the marks already there. Esc or a lost pointer puts
// the old marks back. Near the bottom edge, or just under the sticky strip, the grid scrolls
// by itself and the box follows.
import { onBeforeUnmount, ref, type Ref } from 'vue'
import { boxBetween, boxHits, boxMarks, edgeScrollStep, type Box } from '@/lib/windowsSelection'

const THRESHOLD = 4
const EDGE_ZONE = 32
const EDGE_MAX = 16

export function useWindowsMarquee(options: {
  /** The grid's scroll area; the box is drawn in its content coordinates. */
  scroller: Ref<HTMLElement | null>
  marks: () => ReadonlySet<string>
  setMarks: (marks: Set<string>) => void
}) {
  /** The box on screen, in the scroll area's content coordinates; null while not boxing. */
  const box = ref<Box | null>(null)
  let press: { x: number; y: number; clientX: number; clientY: number; additive: boolean; before: Set<string> } | null = null
  let pointer = { x: 0, y: 0 }
  let frame = 0

  function toContent(el: HTMLElement, clientX: number, clientY: number) {
    const r = el.getBoundingClientRect()
    return { x: clientX - r.left + el.scrollLeft, y: clientY - r.top + el.scrollTop }
  }

  function update() {
    const el = options.scroller.value
    if (!el || !press || !box.value) return
    const p = toContent(el, pointer.x, pointer.y)
    // Kept inside the content, so the box stops at the grid's edges.
    const x = Math.min(Math.max(p.x, 0), el.scrollWidth)
    const y = Math.min(Math.max(p.y, 0), el.scrollHeight)
    box.value = boxBetween(press.x, press.y, x, y)
    const cards = [...el.querySelectorAll<HTMLElement>('[data-window-label]')].map((card) => {
      const b = card.getBoundingClientRect()
      const tl = toContent(el, b.left, b.top)
      return { label: card.dataset.windowLabel ?? '', rect: { left: tl.x, top: tl.y, right: tl.x + b.width, bottom: tl.y + b.height } }
    })
    options.setMarks(boxMarks(press.before, boxHits(cards, box.value), press.additive))
  }

  function autoScroll() {
    frame = 0
    const el = options.scroller.value
    if (!el || !box.value) return
    const r = el.getBoundingClientRect()
    // The strip sticks over the top of the grid: the top edge is where it ends.
    const strip = el.querySelector('[data-windows-drop]')?.getBoundingClientRect()
    const step = edgeScrollStep(pointer.y, Math.max(r.top, strip?.bottom ?? r.top), r.bottom, EDGE_ZONE, EDGE_MAX)
    if (step) el.scrollTop += step
    frame = requestAnimationFrame(autoScroll)
  }

  function stop() {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
    window.removeEventListener('pointercancel', cancel)
    window.removeEventListener('blur', cancel)
    window.removeEventListener('keydown', onKey, true)
    options.scroller.value?.removeEventListener('scroll', update)
    if (frame) cancelAnimationFrame(frame)
    frame = 0
    press = null
    box.value = null
  }

  function onMove(e: PointerEvent) {
    if (!press) return
    pointer = { x: e.clientX, y: e.clientY }
    if (!box.value) {
      if (Math.hypot(e.clientX - press.clientX, e.clientY - press.clientY) < THRESHOLD) return
      box.value = boxBetween(press.x, press.y, press.x, press.y)
      options.scroller.value?.addEventListener('scroll', update)
      frame = requestAnimationFrame(autoScroll)
    }
    update()
  }

  function onUp() {
    stop()
  }

  /** Esc, a cancelled pointer or the window losing focus: the marks from before the box. */
  function cancel() {
    if (press && box.value) options.setMarks(press.before)
    stop()
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !box.value) return
    e.preventDefault()
    e.stopPropagation()
    cancel()
  }

  function onPointerDown(e: PointerEvent) {
    const el = options.scroller.value
    const target = e.target as Element | null
    if (!el || !target || e.button !== 0) return
    // A card is dragged to the strip; buttons, links and the strip keep their own press.
    if (target.closest('[data-window-label], button, input, a, [data-windows-drop]')) return
    const r = el.getBoundingClientRect()
    if (e.clientX - r.left >= el.clientLeft + el.clientWidth) return // the scrollbar
    // The strip's opaque backing over the top of the grid is not grid space either.
    const strip = el.querySelector('[data-windows-drop]')?.getBoundingClientRect()
    if (strip && e.clientY < strip.bottom) return
    // No text selection starts under the box.
    e.preventDefault()
    const p = toContent(el, e.clientX, e.clientY)
    press = { ...p, clientX: e.clientX, clientY: e.clientY, additive: e.shiftKey || e.metaKey || e.ctrlKey, before: new Set(options.marks()) }
    pointer = { x: e.clientX, y: e.clientY }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
    window.addEventListener('pointercancel', cancel)
    window.addEventListener('blur', cancel)
    window.addEventListener('keydown', onKey, true)
  }

  onBeforeUnmount(stop)

  return { box, onPointerDown }
}
