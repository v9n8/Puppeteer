// Drag a card of the week view to another day (V3-39). Tauri's webview keeps
// native HTML5 drag-and-drop for files dropped from the OS, so this runs on
// pointer events (as composables/useDragSort.ts does). A press that moves under
// the threshold stays a click (the row opens the panel). Esc, or a drop outside
// the day columns, cancels. The card follows the pointer by `transform` only —
// its place in the grid changes when the write lands and the board reloads.
import { onBeforeUnmount, ref } from 'vue'

const THRESHOLD = 4
/** Day columns carry this attribute with their `YYYY-MM-DD`. */
const DAY_ATTR = 'data-week-date'

export interface UseWeekDragOptions {
  canDrag: (id: string) => boolean
  onDrop: (id: string, date: string) => void
}

export function useWeekDrag({ canDrag, onDrop }: UseWeekDragOptions) {
  const dragId = ref<string | null>(null)
  const overDate = ref<string | null>(null)
  const dx = ref(0)
  const dy = ref(0)

  let pressed: { id: string; x: number; y: number } | null = null
  let dragged = false

  function dayAt(x: number, y: number): string | null {
    const col = document.elementFromPoint(x, y)?.closest(`[${DAY_ATTR}]`)
    return col?.getAttribute(DAY_ATTR) ?? null
  }

  function onMove(e: PointerEvent) {
    if (!pressed) return
    const mx = e.clientX - pressed.x
    const my = e.clientY - pressed.y
    if (dragId.value === null) {
      if (Math.hypot(mx, my) < THRESHOLD) return
      dragId.value = pressed.id
      dragged = true
      document.body.style.userSelect = 'none'
    }
    dx.value = mx
    dy.value = my
    overDate.value = dayAt(e.clientX, e.clientY)
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || dragId.value === null) return
    e.preventDefault()
    e.stopImmediatePropagation()
    stop()
  }

  function stop() {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', end)
    window.removeEventListener('pointercancel', stop)
    window.removeEventListener('keydown', onKey, true)
    document.body.style.userSelect = ''
    pressed = null
    // The click that closes a drag arrives right after pointerup; one that never comes must not eat the next.
    if (dragged) setTimeout(() => (dragged = false), 0)
    dragId.value = null
    overDate.value = null
    dx.value = 0
    dy.value = 0
  }

  function end(e: PointerEvent) {
    const id = dragId.value
    const date = id === null ? null : dayAt(e.clientX, e.clientY)
    stop()
    if (id !== null && date !== null) onDrop(id, date)
  }

  /** `@pointerdown` on a card; presses on a button or link inside it are theirs. */
  function start(id: string, e: PointerEvent) {
    if (e.button !== 0 || !canDrag(id)) return
    if ((e.target as HTMLElement | null)?.closest('button, a, input, textarea, select')) return
    pressed = { id, x: e.clientX, y: e.clientY }
    dragged = false
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', end)
    window.addEventListener('pointercancel', stop)
    window.addEventListener('keydown', onKey, true)
  }

  /** `@click.capture` on a card: the click that ends a drag must not open the card. */
  function swallowDragClick(e: Event) {
    if (!dragged) return
    dragged = false
    e.stopPropagation()
    e.preventDefault()
  }

  /** The dragged card tracks the pointer and ignores it (so the column under it can be found). */
  function cardStyle(id: string) {
    if (dragId.value !== id) return undefined
    return { transform: `translate(${dx.value}px, ${dy.value}px)`, zIndex: '30', pointerEvents: 'none' as const, opacity: '0.9' }
  }

  onBeforeUnmount(stop)

  return { dragId, overDate, start, swallowDragClick, cardStyle }
}
