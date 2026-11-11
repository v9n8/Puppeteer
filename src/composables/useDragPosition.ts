// Moving a floating panel by a handle and resizing it by a corner, with the
// pointer (QuoteNotePopover; small enough to reuse for other popups). Pointer
// capture keeps the drag going when the pointer leaves the handle; every step
// is clamped so the panel stays 8px inside the window (lib/dragPosition).
import { ref, type Ref } from 'vue'
import { clampPosition, clampSize, type Point, type Size, type SizeLimits } from '@/lib/dragPosition'

/** `pos`: the panel's top-left, set by the caller to open it somewhere. */
export function useDragPosition(panel: Ref<HTMLElement | null>, limits: SizeLimits) {
  const pos = ref<Point>({ x: 0, y: 0 })
  /** Null until the user resizes: the panel keeps its natural size. */
  const size = ref<Size | null>(null)

  const viewport = (): Size => ({ width: window.innerWidth, height: window.innerHeight })
  const measured = (): Size => ({ width: panel.value?.offsetWidth ?? 0, height: panel.value?.offsetHeight ?? 0 })

  function track(e: PointerEvent, move: (dx: number, dy: number) => void) {
    if (e.button !== 0) return
    const handle = e.currentTarget as HTMLElement
    const from = { x: e.clientX, y: e.clientY }
    // No text selection, and the note field keeps its focus.
    e.preventDefault()
    handle.setPointerCapture(e.pointerId)
    const onMove = (ev: PointerEvent) => move(ev.clientX - from.x, ev.clientY - from.y)
    const onEnd = () => {
      handle.removeEventListener('pointermove', onMove)
      handle.removeEventListener('pointerup', onEnd)
      handle.removeEventListener('pointercancel', onEnd)
    }
    handle.addEventListener('pointermove', onMove)
    handle.addEventListener('pointerup', onEnd)
    handle.addEventListener('pointercancel', onEnd)
  }

  function startMove(e: PointerEvent) {
    const start = pos.value
    const box = measured()
    track(e, (dx, dy) => {
      pos.value = clampPosition({ x: start.x + dx, y: start.y + dy }, box, viewport())
    })
  }

  function startResize(e: PointerEvent) {
    const start = measured()
    track(e, (dx, dy) => {
      size.value = clampSize({ width: start.width + dx, height: start.height + dy }, pos.value, viewport(), limits)
    })
  }

  return { pos, size, startMove, startResize }
}
