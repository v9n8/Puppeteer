// Jump between the user's own messages in a long conversation.
//
// Reading back through a run means hunting for "where did I ask for this?" —
// the user turns are the landmarks, everything between them is the assistant
// working. This walks the scroll container from landmark to landmark.
//
// The turns are found in the DOM (`[data-user-turn]`, stamped by StreamLog)
// rather than measured from a list of entry heights: StreamLog skips off-screen
// entries with `content-visibility` but reserves their measured height, so a
// rect read is accurate for entries that were never laid out — an offset table
// we maintained ourselves would not be.
//
// Stepping and the jump list read layout only on demand (a click, a shortcut,
// opening the list). The one reader that follows the scroll — the `n/N` counter
// — calls `activeEntryIndex` at most once per frame, and that is a binary
// search over the turns, so it reads a handful of rects rather than walking
// every turn: the output pane already coalesces its own per-frame layout reads
// to stay smooth, and a full walk per frame would undo that.
import { computed, type Ref } from 'vue'
import type { StreamEntry } from '@/lib/streamEvents'
import { turnIndexAt } from '@/lib/turnPosition'

export interface PromptTurn {
  /** Index into the `entries` array — matches the `data-user-turn` attribute. */
  entryIndex: number
  /** One-line preview for the jump list; empty when the message was images only. */
  label: string
}

/** Gap kept above a turn after jumping, so the bubble isn't flush to the edge. */
const TOP_GAP = 12
/** Slack around the anchor line so the turn just jumped to isn't picked again. */
const EPS = 4
const LABEL_MAX = 160

/** One-line jump-list preview for a user prompt. Exported so the whole-session
 *  scan (which parses records outside this composable) labels turns identically. */
export function turnLabel(text: string): string {
  const flat = text.replace(/\s+/g, ' ').trim()
  return flat.length > LABEL_MAX ? `${flat.slice(0, LABEL_MAX)}…` : flat
}

export function useTurnNavigator(
  scrollEl: Ref<HTMLElement | null>,
  entries: Ref<readonly StreamEntry[]>,
) {
  const turns = computed<PromptTurn[]>(() => {
    const out: PromptTurn[] = []
    entries.value.forEach((e, entryIndex) => {
      if (e.kind === 'user') out.push({ entryIndex, label: turnLabel(e.text) })
    })
    return out
  })

  // Turn elements in document order, paired with how far below the container's
  // top edge each one currently sits.
  function positions(): { entryIndex: number; el: HTMLElement; top: number }[] {
    const root = scrollEl.value
    if (!root) return []
    const base = root.getBoundingClientRect().top
    return Array.from(root.querySelectorAll<HTMLElement>('[data-user-turn]')).map((el) => ({
      entryIndex: Number(el.dataset.userTurn),
      el,
      top: el.getBoundingClientRect().top - base,
    }))
  }

  function scrollTo(el: HTMLElement, smooth = false) {
    const root = scrollEl.value
    if (!root) return
    // Delta from the container's own rect rather than `offsetTop`: the entry's
    // offsetParent isn't necessarily the scroll container.
    const delta = el.getBoundingClientRect().top - root.getBoundingClientRect().top
    const target = Math.max(0, root.scrollTop + delta - TOP_GAP)
    // Stepping (prev/next) lands instantly on purpose — a second press mid-
    // animation would fight it. An explicit pick asks for a smooth glide, but
    // smooth-scrolling the full height of a long run crawls for seconds, so we
    // teleport to within ~1.5 screens of the target first and animate only the
    // last stretch — a consistent, quick landing however far the prompt is.
    if (!smooth) {
      root.scrollTop = target
      return
    }
    const maxAnim = root.clientHeight * 1.5
    if (Math.abs(target - root.scrollTop) > maxAnim) {
      root.scrollTop = target + (target < root.scrollTop ? maxAnim : -maxAnim)
    }
    root.scrollTo({ top: target, behavior: 'smooth' })
  }

  function scrollToTurn(entryIndex: number, smooth = false) {
    const el = scrollEl.value?.querySelector<HTMLElement>(`[data-user-turn="${entryIndex}"]`)
    if (el) scrollTo(el, smooth)
  }

  /** The turn the reader is currently parked on, or -1 when above the first. */
  function activeEntryIndex(): number {
    const root = scrollEl.value
    if (!root) return -1
    const els = root.querySelectorAll<HTMLElement>('[data-user-turn]')
    const base = root.getBoundingClientRect().top
    const i = turnIndexAt(els.length, (k) => els[k].getBoundingClientRect().top - base, TOP_GAP + EPS)
    return i < 0 ? -1 : Number(els[i].dataset.userTurn)
  }

  /** How far below the container's top edge the first loaded turn sits, or null
   *  when no turn is rendered. Negative once it has scrolled out of view. */
  function firstTurnTop(): number | null {
    const root = scrollEl.value
    const el = root?.querySelector<HTMLElement>('[data-user-turn]')
    if (!root || !el) return null
    return el.getBoundingClientRect().top - root.getBoundingClientRect().top
  }

  function goPrev() {
    const above = positions().filter((p) => p.top < TOP_GAP - EPS)
    const target = above[above.length - 1]
    if (target) scrollTo(target.el)
  }

  function goNext() {
    const target = positions().find((p) => p.top > TOP_GAP + EPS)
    if (target) scrollTo(target.el)
  }

  return { turns, scrollToTurn, activeEntryIndex, firstTurnTop, goPrev, goNext }
}
