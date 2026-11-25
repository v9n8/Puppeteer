// The cards marked on the board, so a drag can take them all along. A box drawn on
// empty space marks them (useBoardMarquee). The marks only cover cards on screen: a
// card that leaves it (filter, reload, folded "No flag") loses its mark for good, so
// a group never carries a card the user cannot see; another Project clears them all.
// Esc clears them when nothing else answers it.
import { onBeforeUnmount, onMounted, ref, watch, type Ref } from 'vue'
import type { BoardRun } from '@/stores/flagBoard'
import { hasOpenModal } from '@/lib/modalStack'
import { pruneMarks } from '@/lib/windowsSelection'
import { useBoardMarquee } from './useBoardMarquee'

export function useBoardMarks(options: {
  /** The board's scroll area, where a box can start. */
  scroller: Ref<HTMLElement | null>
  /** The cards on screen, in display order. */
  visible: Readonly<Ref<BoardRun[]>>
  /** The Project on screen ('all' or an id). */
  choice: Readonly<Ref<string>>
  /** A drag, a lifted card or the filter popover answers Esc now. */
  busy: () => boolean
  /** The card menu is open. */
  menuOpen: () => boolean
}) {
  /** Ids of the marked cards. */
  const marks = ref<Set<string>>(new Set())

  watch(options.choice, () => { marks.value = new Set() })
  watch(options.visible, (cards) => {
    const kept = pruneMarks(marks.value, cards.map((r) => ({ label: r.id })))
    if (kept.size !== marks.value.size) marks.value = kept
  })

  const { box, onPointerDown } = useBoardMarquee({
    scroller: options.scroller,
    marks: () => marks.value,
    // Cut to the cards on screen: the marks from before the press may name a card gone since.
    setMarks: (next) => { marks.value = pruneMarks(next, options.visible.value.map((r) => ({ label: r.id }))) },
  })

  /** The cards that travel with `run`: every marked card in display order when it is one, else `run` alone. */
  function groupOf(run: BoardRun): BoardRun[] {
    return marks.value.has(run.id) ? options.visible.value.filter((r) => marks.value.has(r.id)) : [run]
  }

  // Whoever owns Esc sees it first: the drag, the box and the filter popover stop it, the
  // Project menu and the keyboard lift prevent it, dialogs hold the focus. The card menu
  // closes on Esc in its own capture listener without taking the key, so whether it (or a
  // dialog) was open is noted here in the capture phase, before it closes.
  let escTaken = false
  function noteEscOwner(e: KeyboardEvent) {
    if (e.key === 'Escape') escTaken = options.menuOpen() || hasOpenModal()
  }
  function onEscape(e: KeyboardEvent) {
    if (e.key !== 'Escape' || e.defaultPrevented || escTaken || !marks.value.size) return
    if (box.value || options.busy() || hasOpenModal()) return
    const el = e.target instanceof HTMLElement ? e.target : null
    if (el && (el.isContentEditable || el.closest('input, textarea, select, [role="menu"], [role="dialog"]'))) return
    e.preventDefault()
    marks.value = new Set()
  }
  onMounted(() => {
    window.addEventListener('keydown', noteEscOwner, true)
    window.addEventListener('keydown', onEscape)
  })
  onBeforeUnmount(() => {
    window.removeEventListener('keydown', noteEscOwner, true)
    window.removeEventListener('keydown', onEscape)
  })

  return { marks, box, onPointerDown, groupOf }
}
