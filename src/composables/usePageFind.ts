// In-page find (⌘F) over one area of the page: the text nodes under `root` are
// searched (lib/pageFind) and the matches painted with the CSS Custom Highlight
// API, so the DOM is never mutated — this coexists with v-html re-renders and
// shiki token spans. Where the API is missing, matches are still counted and
// scrolled to, just not painted. The bar itself is components/FindBar.vue.
//
// Text under a `.search-skip` element (a line-number gutter) is left out.
//
// Every area registers itself; ⌘F is caught once per window (useFindShortcut,
// from App.vue) and opens the bar of the area being read (lib/pageFind
// `pickFindArea`). The registry is module state: each webview has its own copy.
import { computed, nextTick, onMounted, onUnmounted, ref, watch, type ComputedRef, type Ref } from 'vue'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { FIND_IN_PAGE_FEATURE, findMatches, isFindShortcut, pickFindArea, stepMatch } from '@/lib/pageFind'
import { isSelectionExcludedWindow } from '@/lib/selectionSurface'
import { useFeaturesStore } from '@/stores/features'

/** Global names: `::highlight()` styles for them live in FindBar.vue. */
const HL_ALL = 'page-find'
const HL_CURRENT = 'page-find-current'
const HL_SUPPORTED = typeof CSS !== 'undefined' && 'highlights' in CSS
const DEBOUNCE_MS = 120

export interface PageFind {
  /** `text_tools.find_in_page` is on; `openFind` does nothing otherwise. */
  enabled: ComputedRef<boolean>
  open: Ref<boolean>
  query: Ref<string>
  caseSensitive: Ref<boolean>
  count: Ref<number>
  /** 1-based; 0 when there are no matches. */
  current: Ref<number>
  /** Bound by FindBar; focused on every `openFind`. */
  inputEl: Ref<HTMLInputElement | null>
  openFind: () => void
  closeFind: () => void
  step: (delta: number) => void
  /** Search again after the content under `root` was re-rendered (no-op while closed). */
  refresh: () => void
}

function collectTextNodes(root: HTMLElement): Text[] {
  const nodes: Text[] = []
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      if (!node.nodeValue || !node.nodeValue.trim()) return NodeFilter.FILTER_REJECT
      if (node.parentElement?.closest('.search-skip')) return NodeFilter.FILTER_REJECT
      return NodeFilter.FILTER_ACCEPT
    },
  })
  let n = walker.nextNode()
  while (n) { nodes.push(n as Text); n = walker.nextNode() }
  return nodes
}

interface FindArea {
  root: Ref<HTMLElement | null>
  find: PageFind
}

/** Find areas mounted in this window, in mount order. */
const findAreas: FindArea[] = []

export function usePageFind(root: Ref<HTMLElement | null>): PageFind {
  const features = useFeaturesStore()
  const enabled = computed(() => features.isOn(FIND_IN_PAGE_FEATURE))
  const open = ref(false)
  const query = ref('')
  const caseSensitive = ref(false)
  const count = ref(0)
  const current = ref(0)
  const inputEl = ref<HTMLInputElement | null>(null)
  let ranges: Range[] = []
  // The highlights this instance registered, so clearing never removes another
  // area's (the registry is shared by the whole document).
  let hlAll: Highlight | null = null
  let hlCurrent: Highlight | null = null

  function clearHighlights() {
    if (!HL_SUPPORTED) return
    if (hlAll && CSS.highlights.get(HL_ALL) === hlAll) CSS.highlights.delete(HL_ALL)
    if (hlCurrent && CSS.highlights.get(HL_CURRENT) === hlCurrent) CSS.highlights.delete(HL_CURRENT)
    hlAll = null
    hlCurrent = null
  }

  function paintHighlights() {
    if (!HL_SUPPORTED) return
    clearHighlights()
    if (!ranges.length) return
    hlAll = new Highlight(...ranges)
    CSS.highlights.set(HL_ALL, hlAll)
    const cur = ranges[current.value - 1]
    if (cur) {
      hlCurrent = new Highlight(cur)
      hlCurrent.priority = 1
      CSS.highlights.set(HL_CURRENT, hlCurrent)
    }
  }

  function scrollToMatch() {
    const range = ranges[current.value - 1]
    range?.startContainer.parentElement?.scrollIntoView({ block: 'center', inline: 'nearest' })
  }

  function runSearch() {
    ranges = []
    const el = root.value
    if (!query.value || !el) {
      count.value = 0
      current.value = 0
      clearHighlights()
      return
    }
    const nodes = collectTextNodes(el)
    const matches = findMatches(nodes.map((n) => n.nodeValue ?? ''), query.value, {
      caseSensitive: caseSensitive.value,
    })
    for (const m of matches) {
      const range = document.createRange()
      range.setStart(nodes[m.chunk], m.start)
      range.setEnd(nodes[m.chunk], m.end)
      ranges.push(range)
    }
    count.value = ranges.length
    current.value = ranges.length ? 1 : 0
    paintHighlights()
    scrollToMatch()
  }

  function step(delta: number) {
    if (!count.value) return
    current.value = stepMatch(current.value, count.value, delta)
    paintHighlights()
    scrollToMatch()
  }

  function openFind() {
    if (!enabled.value) return
    open.value = true
    nextTick(() => {
      inputEl.value?.focus()
      inputEl.value?.select()
      if (query.value) runSearch()
    })
  }

  function closeFind() {
    open.value = false
    clearHighlights()
  }

  // Old Range objects point at detached nodes once the content re-renders.
  function refresh() {
    if (open.value) nextTick(runSearch)
  }

  // Debounced re-search as the user types.
  let timer: ReturnType<typeof setTimeout> | null = null
  watch(query, () => {
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => nextTick(runSearch), DEBOUNCE_MS)
  })
  watch(caseSensitive, refresh)
  watch(enabled, (on) => { if (!on) closeFind() })

  const find: PageFind = { enabled, open, query, caseSensitive, count, current, inputEl, openFind, closeFind, step, refresh }
  const area: FindArea = { root, find }
  findAreas.push(area)

  onUnmounted(() => {
    if (timer) clearTimeout(timer)
    clearHighlights()
    const i = findAreas.indexOf(area)
    if (i !== -1) findAreas.splice(i, 1)
  })

  return find
}

/** The root while it is on screen: not in a cached (KeepAlive) view, not `display: none`. */
function shownRoot(area: FindArea): HTMLElement | null {
  const el = area.root.value
  return el && el.isConnected && el.getClientRects().length > 0 ? el : null
}

function windowLabel(): string | null {
  try {
    return getCurrentWebviewWindow().label
  } catch {
    return null
  }
}

/** Route ⌘F / Ctrl+F to the find area being read. Call once per window. */
export function useFindShortcut(): void {
  // The Secret vault: search never touches its text.
  const excludedWindow = isSelectionExcludedWindow(window.location.search, windowLabel())
  let lastPointer: Node | null = null

  function onPointerDown(e: PointerEvent) {
    lastPointer = e.target as Node | null
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.defaultPrevented || !isFindShortcut(e)) return
    const overlays = document.querySelectorAll('.ui-backdrop')
    const area = pickFindArea(findAreas, shownRoot, {
      excludedWindow,
      overlay: overlays[overlays.length - 1] ?? null,
      hints: [document.activeElement, lastPointer],
    })
    if (!area || !area.find.enabled.value) return
    e.preventDefault()
    area.find.openFind()
  }

  onMounted(() => {
    window.addEventListener('pointerdown', onPointerDown, true)
    window.addEventListener('keydown', onKeydown)
  })
  onUnmounted(() => {
    window.removeEventListener('pointerdown', onPointerDown, true)
    window.removeEventListener('keydown', onKeydown)
  })
}
