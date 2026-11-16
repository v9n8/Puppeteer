// Dragging a board card with the pointer. The webview keeps native HTML5 drag for
// dropping files, so (like useDragSort) this runs on raw pointer events: a press
// that travels a few pixels becomes a drag, the column under the pointer is found
// from `data-board-column`, and the board scrolls sideways near its edges.
import { onBeforeUnmount, ref, type Ref } from 'vue'
import type { BoardRun } from '@/stores/flagBoard'

const THRESHOLD = 4
const EDGE = 56
const SCROLL_STEP = 16

export interface BoardDragState {
  run: BoardRun
  x: number
  y: number
  /** The column under the pointer, if any. */
  over: string | null
}

export function useBoardDrag(options: {
  board: Ref<HTMLElement | null>
  /** Called on release over a column (other than the card's own). */
  onDrop: (run: BoardRun, columnId: string) => void
  /** Whether `run` may land on the column; false shows the "no drop" state. */
  canLand: (run: BoardRun, columnId: string) => boolean
  /** The column the card sits in now. */
  columnOf: (run: BoardRun) => string
}) {
  const drag = ref<BoardDragState | null>(null)
  let press: { run: BoardRun; x: number; y: number } | null = null
  let swallowClick = false
  let raf = 0

  function columnAt(x: number, y: number): string | null {
    const el = document.elementFromPoint(x, y)?.closest<HTMLElement>('[data-board-column]')
    return el?.dataset.boardColumn ?? null
  }

  function scrollNearEdge() {
    const board = options.board.value
    const d = drag.value
    if (!board || !d) return
    const rect = board.getBoundingClientRect()
    if (d.x < rect.left + EDGE) board.scrollLeft -= SCROLL_STEP
    else if (d.x > rect.right - EDGE) board.scrollLeft += SCROLL_STEP
    raf = requestAnimationFrame(scrollNearEdge)
  }

  function stop() {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
    window.removeEventListener('pointercancel', cancel)
    window.removeEventListener('keydown', onKey, true)
    cancelAnimationFrame(raf)
    press = null
    drag.value = null
  }

  function onMove(e: PointerEvent) {
    if (!press) return
    if (!drag.value) {
      if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < THRESHOLD) return
      drag.value = { run: press.run, x: e.clientX, y: e.clientY, over: null }
      raf = requestAnimationFrame(scrollNearEdge)
    }
    drag.value.x = e.clientX
    drag.value.y = e.clientY
    drag.value.over = columnAt(e.clientX, e.clientY)
  }

  function onUp(e: PointerEvent) {
    const d = drag.value
    if (d) {
      swallowClick = true
      setTimeout(() => { swallowClick = false })
      const target = columnAt(e.clientX, e.clientY)
      if (target && target !== options.columnOf(d.run) && options.canLand(d.run, target)) options.onDrop(d.run, target)
    }
    stop()
  }

  function cancel() {
    if (drag.value) {
      swallowClick = true
      setTimeout(() => { swallowClick = false })
    }
    stop()
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !drag.value) return
    e.preventDefault()
    e.stopPropagation()
    cancel()
  }

  function onCardPointerDown(e: PointerEvent, run: BoardRun) {
    if (e.button !== 0) return
    press = { run, x: e.clientX, y: e.clientY }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
    window.addEventListener('pointercancel', cancel)
    window.addEventListener('keydown', onKey, true)
  }

  /** True once after a drag ended: the click that follows the release must not open the card. */
  function consumeClick(): boolean {
    const was = swallowClick
    swallowClick = false
    return was
  }

  onBeforeUnmount(stop)

  return { drag, onCardPointerDown, consumeClick }
}
