// Marking board cards by dragging a box over the board. The press must land on empty
// space: a press on a card drags it (useBoardDrag), buttons keep their click. The
// cards the box touches are marked as it moves (lib/windowsSelection); ⇧ / ⌘ / Ctrl
// held at the press adds them to the marks already there. Esc or a lost pointer puts
// the old marks back. The board scrolls sideways near its edges and the card list
// under the pointer scrolls near its own; the box follows. Each column's list scrolls
// on its own, so a card's vertical hit is measured in its list's content
// (lib/flagBoard listSpan): a card hidden in its list is not hit, and one the box
// went over stays hit when the list scrolls.
import { onBeforeUnmount, ref, type Ref } from 'vue'
import { boxBetween, boxHits, boxMarks, edgeScrollStep, type Box } from '@/lib/windowsSelection'
import { listSpan } from '@/lib/flagBoard'

const THRESHOLD = 4
// Same sideways zone as a card drag (useBoardDrag); the list zone as in the windows overlay.
const EDGE_X = 56
const EDGE_Y = 32
const EDGE_MAX = 16
// Cards drag themselves; buttons (the fold button, the folded "No flag" strip), fields and links keep their press.
const IGNORE = '[data-card-id], button, input, a, label'
const LIST = '[data-board-cards]'

export function useBoardMarquee(options: {
  /** The board's scroll area; the box is drawn in its content coordinates. */
  scroller: Ref<HTMLElement | null>
  marks: () => ReadonlySet<string>
  setMarks: (marks: Set<string>) => void
}) {
  /** The box on screen, in the board's content coordinates; null while not boxing. */
  const box = ref<Box | null>(null)
  let press: {
    x: number
    y: number
    clientX: number
    clientY: number
    additive: boolean
    before: Set<string>
    /** Where each card list was scrolled at the press. */
    lists: Map<Element, number>
  } | null = null
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
    // Kept inside the content, so the box stops at the board's edges.
    const x = Math.min(Math.max(p.x, 0), el.scrollWidth)
    const y = Math.min(Math.max(p.y, 0), el.scrollHeight)
    box.value = boxBetween(press.x, press.y, x, y)
    const left = box.value.left
    const right = box.value.right
    const hits: string[] = []
    for (const list of el.querySelectorAll<HTMLElement>(LIST)) {
      const lr = list.getBoundingClientRect()
      const span = listSpan(press.clientY, pointer.y, lr, press.lists.get(list) ?? list.scrollTop, list.scrollTop)
      if (!span) continue
      // Sideways in the board's content, up and down in the list's.
      const cards = [...list.querySelectorAll<HTMLElement>('[data-card-id]')].map((card) => {
        const b = card.getBoundingClientRect()
        const cardLeft = toContent(el, b.left, 0).x
        const top = b.top - lr.top + list.scrollTop
        return { label: card.dataset.cardId ?? '', rect: { left: cardLeft, right: cardLeft + b.width, top, bottom: top + b.height } }
      })
      hits.push(...boxHits(cards, { left, right, top: span.top, bottom: span.bottom }))
    }
    options.setMarks(boxMarks(press.before, hits, press.additive))
  }

  function autoScroll() {
    frame = 0
    const el = options.scroller.value
    if (!el || !box.value) return
    const r = el.getBoundingClientRect()
    const step = edgeScrollStep(pointer.x, r.left, r.right, EDGE_X, EDGE_MAX)
    if (step) el.scrollLeft += step
    // Only inside the list's band: a pointer resting on the header must not scroll hidden cards under the box.
    const list = document.elementFromPoint(pointer.x, pointer.y)?.closest('[data-board-column]')?.querySelector<HTMLElement>(LIST)
    if (list && el.contains(list)) {
      const lr = list.getBoundingClientRect()
      if (pointer.y >= lr.top && pointer.y <= lr.bottom) {
        const down = edgeScrollStep(pointer.y, lr.top, lr.bottom, EDGE_Y, EDGE_MAX)
        if (down) list.scrollTop += down
      }
    }
    frame = requestAnimationFrame(autoScroll)
  }

  function stop() {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
    window.removeEventListener('pointercancel', cancel)
    window.removeEventListener('blur', cancel)
    window.removeEventListener('keydown', onKey, true)
    options.scroller.value?.removeEventListener('scroll', update, true)
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
      // Capture: a card list's own scroll does not bubble up to the board.
      options.scroller.value?.addEventListener('scroll', update, true)
      frame = requestAnimationFrame(autoScroll)
    }
    update()
  }

  function onUp() {
    // A click on empty space clears the marks; with ⇧ / ⌘ / Ctrl it leaves them.
    if (press && !box.value && !press.additive) options.setMarks(new Set())
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
    if (target.closest(IGNORE)) return
    const r = el.getBoundingClientRect()
    if (e.clientY - r.top >= el.clientTop + el.clientHeight) return // the horizontal scrollbar
    // No text selection starts under the box.
    e.preventDefault()
    const p = toContent(el, e.clientX, e.clientY)
    press = {
      ...p,
      clientX: e.clientX,
      clientY: e.clientY,
      additive: e.shiftKey || e.metaKey || e.ctrlKey,
      before: new Set(options.marks()),
      lists: new Map([...el.querySelectorAll(LIST)].map((list) => [list, list.scrollTop])),
    }
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
