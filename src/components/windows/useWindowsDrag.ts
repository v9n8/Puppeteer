// Dragging cards of the "Open windows" overlay onto its drop strip (WindowsDropStrip) to
// move them back to the main window. The webview keeps native HTML5 drag for dropping
// files, so (like board/useBoardDrag) this runs on raw pointer events: a press that
// travels a few pixels becomes a drag, a shorter one stays a click.
import { computed, onBeforeUnmount, ref } from 'vue'
import type { WindowCard } from '@/lib/windowsOverlay'

const THRESHOLD = 4

export interface WindowsDragState {
  /** The windows dragged: the pressed card, or every marked card when it is one of them. */
  labels: string[]
  /** The pressed card, the one drawn under the pointer. */
  lead: string
  x: number
  y: number
  /** Where the pointer holds the card, so the floating copy keeps the same grip. */
  dx: number
  dy: number
  width: number
}

export function useWindowsDrag(options: {
  /** The cards on screen, in display order. */
  cards: () => readonly WindowCard[]
  marks: () => ReadonlySet<string>
  /** Called on release over the strip when one of the dragged windows can move back. */
  onDrop: (labels: string[]) => void
}) {
  const drag = ref<WindowsDragState | null>(null)
  const dragged = computed(() => options.cards().filter((c) => drag.value?.labels.includes(c.label)))
  const lead = computed(() => options.cards().find((c) => c.label === drag.value?.lead))
  let press: { label: string; x: number; y: number; card: DOMRect } | null = null
  let swallowClick = false

  function overStrip(x: number, y: number): boolean {
    return !!document.elementFromPoint(x, y)?.closest('[data-windows-drop]')
  }

  function stop() {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
    window.removeEventListener('pointercancel', cancel)
    window.removeEventListener('keydown', onKey, true)
    press = null
    drag.value = null
  }

  /** The click that follows the release of a drag must not open or mark the card. */
  function swallowNextClick() {
    swallowClick = true
    setTimeout(() => { swallowClick = false })
  }

  function onMove(e: PointerEvent) {
    if (!press) return
    if (!drag.value) {
      if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < THRESHOLD) return
      drag.value = {
        // A marked card takes every marked card with it.
        labels: options.marks().has(press.label)
          ? options.cards().map((c) => c.label).filter((l) => options.marks().has(l))
          : [press.label],
        lead: press.label,
        x: 0,
        y: 0,
        dx: press.x - press.card.left,
        dy: press.y - press.card.top,
        width: press.card.width,
      }
    }
    drag.value.x = e.clientX
    drag.value.y = e.clientY
  }

  function onUp(e: PointerEvent) {
    const d = drag.value
    if (d) {
      swallowNextClick()
      // None of them can move back: the strip said so, the drop does nothing.
      if (overStrip(e.clientX, e.clientY) && dragged.value.some((c) => c.canMerge)) options.onDrop(d.labels)
    }
    stop()
  }

  function cancel() {
    if (drag.value) swallowNextClick()
    stop()
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !drag.value) return
    e.preventDefault()
    e.stopPropagation()
    cancel()
  }

  function onCardPointerDown(e: PointerEvent, label: string) {
    // The checkbox and the close button stay buttons.
    if (e.button !== 0 || (e.target as Element | null)?.closest('button')) return
    const card = (e.currentTarget as HTMLElement).getBoundingClientRect()
    press = { label, x: e.clientX, y: e.clientY, card }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
    window.addEventListener('pointercancel', cancel)
    window.addEventListener('keydown', onKey, true)
  }

  /** True once after a drag ended. */
  function consumeClick(): boolean {
    const was = swallowClick
    swallowClick = false
    return was
  }

  onBeforeUnmount(stop)

  return { drag, dragged, lead, onCardPointerDown, consumeClick }
}
