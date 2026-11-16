import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { applyFontPrefs, loadFontPrefs, saveFontPrefs, type FontPrefs } from '@/lib/appFonts'
import {
  SHORT_HEIGHT,
  dockModeFor,
  parseDockPrefs,
  toggledDockPrefs,
  type DockMode,
  type DockPrefs,
} from '@/lib/dockMode'
import {
  NARROW_WIDTH,
  parseSidebarPrefs,
  sidebarModeFor,
  toggledPrefs,
  withMode,
  type SidebarMode,
  type SidebarPrefs,
} from '@/lib/sidebarMode'

/**
 * App-wide layout visibility state shared between App.vue and RunView.
 *
 * `focusMode` collapses the app down to just the current AI run: the sidebar,
 * workspace tabs, run history and issue/PR Content are hidden, leaving only
 * Claude's output/question area and the composer. It is intentionally a
 * transient, in-memory flag — it resets on reload rather than persisting.
 *
 * `sidebarMode` is the separate, deliberate "give me more width" toggle: the
 * sidebar shrinks to an icon rail (never disappears, so every screen and badge
 * stays reachable). Unlike focus mode it applies to ALL routes and IS
 * persisted, once for wide windows and once for narrow ones (lib/sidebarMode).
 */
const SIDEBAR_STORAGE_KEY = 'puppeteer.sidebar'
// Pre-rail key: the sidebar was hidden outright. Read once, then removed.
const LEGACY_SIDEBAR_HIDDEN_KEY = 'puppeteer.sidebarHidden'

/** Active runs dock: expanded or one line, kept per window height (lib/dockMode). */
const DOCK_STORAGE_KEY = 'puppeteer.dock'

function loadDockPrefs(): DockPrefs {
  try {
    return parseDockPrefs(localStorage.getItem(DOCK_STORAGE_KEY))
  } catch {
    return parseDockPrefs(null)
  }
}

/**
 * Row density shared by the session lists — the History list (RunView's left
 * rail) and the worker panel. Persisted like `sidebarMode`, since it is a
 * lasting reading preference.
 *  - comfortable: the full multi-line row (badges + timestamp line)
 *  - compact:     two lines — title row + "account · model"
 *  - dense:       one line; the rest lives in the row tooltip
 */
export type ListDensity = 'comfortable' | 'compact' | 'dense'
const LIST_DENSITY_KEY = 'puppeteer.listDensity'
// Pre-rename key (History-only density); read once so the choice carries over.
const LEGACY_HISTORY_DENSITY_KEY = 'puppeteer.historyDensity'
const LIST_DENSITIES: readonly ListDensity[] = ['comfortable', 'compact', 'dense']

function loadListDensity(): ListDensity {
  try {
    let v = localStorage.getItem(LIST_DENSITY_KEY)
    if (v == null) {
      v = localStorage.getItem(LEGACY_HISTORY_DENSITY_KEY)
      if (v != null) {
        localStorage.setItem(LIST_DENSITY_KEY, v)
        localStorage.removeItem(LEGACY_HISTORY_DENSITY_KEY)
      }
    }
    if (v && (LIST_DENSITIES as readonly string[]).includes(v)) return v as ListDensity
  } catch {
    /* storage unavailable — fall through to the default */
  }
  return 'compact'
}

/**
 * Worker-panel timeline: collapsed to a one-line header by default; when open,
 * its height (px) is user-resizable. Both persist across restarts.
 */
const TIMELINE_OPEN_KEY = 'puppeteer.workerTimelineOpen'
const TIMELINE_HEIGHT_KEY = 'puppeteer.workerTimelineHeight'
export const TIMELINE_MIN_HEIGHT = 80
const TIMELINE_DEFAULT_HEIGHT = 192

function loadTimelineOpen(): boolean {
  try {
    return localStorage.getItem(TIMELINE_OPEN_KEY) === '1'
  } catch {
    return false
  }
}

function loadTimelineHeight(): number {
  try {
    const n = Number(localStorage.getItem(TIMELINE_HEIGHT_KEY))
    if (Number.isFinite(n) && n >= TIMELINE_MIN_HEIGHT) return Math.round(n)
  } catch {
    /* storage unavailable — fall through to the default */
  }
  return TIMELINE_DEFAULT_HEIGHT
}

/**
 * Session log: show the steps (tool calls, thinking, commentary) of every turn
 * instead of folding finished turns under a one-line summary (StreamLog
 * `stepsExpanded`). A lasting reading preference, persisted like `listDensity`.
 */
const STEPS_EXPANDED_KEY = 'puppeteer.turnStepsExpanded'

function loadStepsExpanded(): boolean {
  try {
    return localStorage.getItem(STEPS_EXPANDED_KEY) === '1'
  } catch {
    return false
  }
}

/** Markdown file viewer: the Contents column beside the rendered text (V3-25); on by default. */
const FILE_OUTLINE_KEY = 'puppeteer.fileOutlineOpen'

function loadFileOutlineOpen(): boolean {
  try {
    return localStorage.getItem(FILE_OUTLINE_KEY) !== '0'
  } catch {
    return true
  }
}

function loadSidebarPrefs(): SidebarPrefs {
  try {
    const prefs = parseSidebarPrefs(
      localStorage.getItem(SIDEBAR_STORAGE_KEY),
      localStorage.getItem(LEGACY_SIDEBAR_HIDDEN_KEY),
    )
    if (localStorage.getItem(LEGACY_SIDEBAR_HIDDEN_KEY) != null) {
      localStorage.setItem(SIDEBAR_STORAGE_KEY, JSON.stringify(prefs))
      localStorage.removeItem(LEGACY_SIDEBAR_HIDDEN_KEY)
    }
    return prefs
  } catch {
    return parseSidebarPrefs(null)
  }
}

/**
 * Session composer: the height the user dragged it to from its top edge
 * (UX-65), or none (it only grows with its text). Persisted like the timeline's.
 */
const COMPOSER_HEIGHT_KEY = 'puppeteer.composerHeight'

function loadComposerHeight(): number | null {
  try {
    const n = Number(localStorage.getItem(COMPOSER_HEIGHT_KEY))
    return Number.isFinite(n) && n > 0 ? n : null
  } catch {
    return null
  }
}

/** Flag board (V3-22): the Project it shows (an id, 'all', or none yet) and whether its "No flag" column is folded. */
const BOARD_PROJECT_KEY = 'puppeteer.boardProject'
const BOARD_NO_FLAG_KEY = 'puppeteer.boardNoFlagCollapsed'

function loadBoardProject(): string | null {
  try {
    return localStorage.getItem(BOARD_PROJECT_KEY) || null
  } catch {
    return null
  }
}

/** null: never chosen, so the board's own default applies (folded on the All projects board). */
function loadBoardNoFlagCollapsed(): boolean | null {
  try {
    const raw = localStorage.getItem(BOARD_NO_FLAG_KEY)
    return raw === null ? null : raw === '1'
  } catch {
    return null
  }
}

export const useUILayoutStore = defineStore('uiLayout', () => {
  const focusMode = ref(false)
  const sidebarPrefs = ref<SidebarPrefs>(loadSidebarPrefs())
  // Only the narrow/wide side of NARROW_WIDTH matters, so listen to that one
  // media query instead of every resize event.
  const windowWidth = ref(typeof window === 'undefined' ? NARROW_WIDTH : window.innerWidth)
  if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
    window
      .matchMedia(`(max-width: ${NARROW_WIDTH - 1}px)`)
      .addEventListener('change', () => (windowWidth.value = window.innerWidth))
  }
  const sidebarMode = computed<SidebarMode>(() => sidebarModeFor(sidebarPrefs.value, windowWidth.value))

  const dockPrefs = ref<DockPrefs>(loadDockPrefs())
  // Same idea for the height: only the side of SHORT_HEIGHT matters.
  const windowHeight = ref(typeof window === 'undefined' ? SHORT_HEIGHT : window.innerHeight)
  if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
    window
      .matchMedia(`(max-height: ${SHORT_HEIGHT - 1}px)`)
      .addEventListener('change', () => (windowHeight.value = window.innerHeight))
  }
  const dockMode = computed<DockMode>(() => dockModeFor(dockPrefs.value, windowHeight.value))

  function toggleDock() {
    dockPrefs.value = toggledDockPrefs(dockPrefs.value, windowHeight.value)
    try {
      localStorage.setItem(DOCK_STORAGE_KEY, JSON.stringify(dockPrefs.value))
    } catch {
      /* storage unavailable — the in-memory choice still works for this session */
    }
  }

  function toggleFocus() {
    focusMode.value = !focusMode.value
  }

  function saveSidebarPrefs(next: SidebarPrefs) {
    sidebarPrefs.value = next
    try {
      localStorage.setItem(SIDEBAR_STORAGE_KEY, JSON.stringify(next))
    } catch {
      /* storage unavailable — the in-memory choice still works for this session */
    }
  }

  function toggleSidebar() {
    saveSidebarPrefs(toggledPrefs(sidebarPrefs.value, windowWidth.value))
  }

  function expandSidebar() {
    saveSidebarPrefs(withMode(sidebarPrefs.value, windowWidth.value, 'full'))
  }

  const listDensity = ref<ListDensity>(loadListDensity())

  function setListDensity(density: ListDensity) {
    listDensity.value = density
    try {
      localStorage.setItem(LIST_DENSITY_KEY, density)
    } catch {
      /* storage unavailable — the in-memory value still works for this session */
    }
  }

  const boardProject = ref<string | null>(loadBoardProject())
  const boardNoFlagCollapsed = ref<boolean | null>(loadBoardNoFlagCollapsed())

  function setBoardProject(project: string | null) {
    boardProject.value = project
    try {
      if (project) localStorage.setItem(BOARD_PROJECT_KEY, project)
      else localStorage.removeItem(BOARD_PROJECT_KEY)
    } catch {
      /* storage unavailable — the in-memory value still works for this session */
    }
  }

  function setBoardNoFlagCollapsed(collapsed: boolean) {
    boardNoFlagCollapsed.value = collapsed
    try {
      localStorage.setItem(BOARD_NO_FLAG_KEY, collapsed ? '1' : '0')
    } catch {
      /* storage unavailable — the in-memory value still works for this session */
    }
  }

  const timelineOpen = ref(loadTimelineOpen())
  const timelineHeight = ref(loadTimelineHeight())

  function setTimelineOpen(open: boolean) {
    timelineOpen.value = open
    try {
      if (open) localStorage.setItem(TIMELINE_OPEN_KEY, '1')
      else localStorage.removeItem(TIMELINE_OPEN_KEY)
    } catch {
      /* storage unavailable — the in-memory flag still works for this session */
    }
  }

  /** Callers clamp the upper bound (it depends on the panel's height). */
  function setTimelineHeight(px: number) {
    timelineHeight.value = Math.max(TIMELINE_MIN_HEIGHT, Math.round(px))
    try {
      localStorage.setItem(TIMELINE_HEIGHT_KEY, String(timelineHeight.value))
    } catch {
      /* storage unavailable — the in-memory value still works for this session */
    }
  }

  const stepsExpanded = ref(loadStepsExpanded())

  function setStepsExpanded(expanded: boolean) {
    stepsExpanded.value = expanded
    try {
      if (expanded) localStorage.setItem(STEPS_EXPANDED_KEY, '1')
      else localStorage.removeItem(STEPS_EXPANDED_KEY)
    } catch {
      /* storage unavailable — the in-memory flag still works for this session */
    }
  }

  const fileOutlineOpen = ref(loadFileOutlineOpen())

  function setFileOutlineOpen(open: boolean) {
    fileOutlineOpen.value = open
    try {
      if (open) localStorage.removeItem(FILE_OUTLINE_KEY)
      else localStorage.setItem(FILE_OUTLINE_KEY, '0')
    } catch {
      /* storage unavailable — the in-memory flag still works for this session */
    }
  }

  const composerHeight = ref<number | null>(loadComposerHeight())

  function setComposerHeight(px: number | null) {
    composerHeight.value = px
    try {
      if (px == null) localStorage.removeItem(COMPOSER_HEIGHT_KEY)
      else localStorage.setItem(COMPOSER_HEIGHT_KEY, String(px))
    } catch {
      /* storage unavailable — the in-memory value still works for this session */
    }
  }

  // Font and text sizes (lib/appFonts): applied on change; other windows follow
  // through the storage event.
  const fontPrefs = ref<FontPrefs>(loadFontPrefs())

  function setFontPrefs(patch: Partial<FontPrefs>) {
    fontPrefs.value = { ...fontPrefs.value, ...patch }
    saveFontPrefs(fontPrefs.value)
    applyFontPrefs(fontPrefs.value)
  }

  return {
    focusMode,
    toggleFocus,
    windowWidth,
    sidebarMode,
    toggleSidebar,
    expandSidebar,
    dockMode,
    toggleDock,
    listDensity,
    setListDensity,
    boardProject,
    setBoardProject,
    boardNoFlagCollapsed,
    setBoardNoFlagCollapsed,
    timelineOpen,
    setTimelineOpen,
    timelineHeight,
    setTimelineHeight,
    stepsExpanded,
    setStepsExpanded,
    composerHeight,
    setComposerHeight,
    fileOutlineOpen,
    setFileOutlineOpen,
    fontPrefs,
    setFontPrefs,
  }
})
