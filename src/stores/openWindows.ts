import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { getAllWebviewWindows, getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { invoke as localInvoke } from '@tauri-real/core'
import { i18n } from '@/i18n'
import { IS_MAC } from '@/lib/appMenu'
import { isRemoteMode } from '@/lib/ipc/remote'
import {
  THUMB_WIDTH,
  isSnapshotAllowed,
  shouldShowBanner,
  type SnapshotResult,
  type ThumbEntry,
  type ThumbStatus,
} from '@/lib/windowThumbs'
import { invoke } from '@/lib/tauri'
import { labelFor } from '@/lib/tray'
import {
  loadRunStatuses,
  loadWindowMeta,
  markWindowUsed,
  pruneWindowMeta,
} from '@/lib/windowRegistry'
import {
  WINDOWS_CHANGED,
  cardsFrom,
  kindOfLabel,
  type WindowInfo,
  type WindowMetaMap,
  type WindowRunStatus,
} from '@/lib/windowsOverlay'
import { useProjectsStore } from '@/stores/projects'

// Backup for a window whose opener closed first (nobody left to broadcast its end).
const POLL_MS = 3000

/**
 * The private windows (every window but the main one and the transient ones) of this
 * machine, for the "Open windows" overlay and the titlebar button's count. Recounted on
 * the `windows:changed` broadcast, when the stored window meta / run badges change, when
 * this window gets focus and every few seconds. A remote-desktop window lists nothing:
 * the windows of this machine are not its business.
 */
export const useOpenWindowsStore = defineStore('openWindows', () => {
  const windows = ref<WindowInfo[]>([])
  const meta = ref<WindowMetaMap>({})
  const statuses = ref<Record<string, WindowRunStatus>>({})
  const here = ref('')
  /** The overlay is showing in THIS window. */
  const open = ref(false)
  /** Bumped by "open" while already open: the overlay selects the next card. */
  const cycleTick = ref(0)
  // Thumbnails (macOS): kept between openings so a card never blinks; shots are re-taken each time.
  const thumbs = ref<Record<string, ThumbEntry>>({})
  const thumbStatus = ref<Record<string, ThumbStatus>>({})
  const thumbsFailed = ref(false)
  let shooting = false

  const projects = useProjectsStore()
  const t = (key: string) => i18n.global.t(key)

  const cards = computed(() =>
    cardsFrom(windows.value, meta.value, statuses.value, here.value, {
      nameOf: labelFor,
      projectNameOf: (id) => projects.projects.find((p) => p.id === id)?.name ?? '',
      screenSub: (label, title) => {
        if (kindOfLabel(label) === 'remote') return title.trim() && title.trim() !== 'Puppeteer' ? title.trim() : ''
        return t('windows.group.screen')
      },
    }),
  )
  const count = computed(() => cards.value.length)

  async function refresh(): Promise<void> {
    if (isRemoteMode()) return
    try {
      const all = await getAllWebviewWindows()
      const infos: WindowInfo[] = []
      for (const w of all) {
        if (!kindOfLabel(w.label)) continue
        let title = ''
        let minimized = false
        try {
          title = await w.title()
          minimized = await w.isMinimized()
        } catch {
          /* window may be mid-teardown */
        }
        infos.push({ label: w.label, title, minimized })
      }
      pruneWindowMeta(all.map((w) => w.label))
      windows.value = infos
      meta.value = loadWindowMeta()
      statuses.value = loadRunStatuses()
    } catch {
      /* outside the Tauri shell (browser dev): no other windows */
    }
  }

  /**
   * Takes a thumbnail of each window, one after the other, so the cards fill in turn and the
   * keyboard is never waiting on them. Windows / Linux: nothing to shoot, the cards keep their icons.
   */
  async function refreshThumbs(): Promise<void> {
    if (!IS_MAC || shooting) return
    shooting = true
    const labels = cards.value.map((c) => c.label).filter(isSnapshotAllowed)
    // A closed window's picture goes; the others stay until replaced.
    thumbs.value = Object.fromEntries(Object.entries(thumbs.value).filter(([l]) => labels.includes(l)))
    thumbStatus.value = Object.fromEntries(labels.map((l) => [l, 'loading' as const]))
    const run = isRemoteMode() ? localInvoke : invoke
    const results: SnapshotResult[] = []
    try {
      for (const label of labels) {
        if (!open.value) break
        let res: SnapshotResult
        try {
          const [first] = await run<SnapshotResult[]>('snapshot_windows', { labels: [label], width: THUMB_WIDTH })
          res = first ?? { label, error: 'empty' }
        } catch (e) {
          res = { label, error: String(e) }
        }
        results.push(res)
        if (res.image) thumbs.value = { ...thumbs.value, [label]: { src: res.image, at: Date.now() } }
        thumbStatus.value = { ...thumbStatus.value, [label]: res.image ? undefined : 'failed' }
      }
      thumbsFailed.value = shouldShowBanner(results)
    } finally {
      // The overlay closed mid-way: nothing is "loading" any more.
      thumbStatus.value = Object.fromEntries(
        Object.entries(thumbStatus.value).map(([l, s]) => [l, s === 'loading' ? undefined : s]),
      )
      shooting = false
    }
  }

  /** The Open-windows command: opens the overlay here, or selects the next card when it is open. */
  function openOrCycle(): void {
    if (open.value) cycleTick.value++
    else open.value = true
  }

  let started = false
  function start(): void {
    if (started || isRemoteMode()) return
    started = true
    try {
      here.value = getCurrentWebviewWindow().label
    } catch {
      /* browser dev */
    }
    const onFocus = () => {
      if (here.value) markWindowUsed(here.value)
      void refresh()
    }
    window.addEventListener('focus', onFocus)
    window.addEventListener('storage', () => void refresh())
    if (document.hasFocus()) onFocus()
    setInterval(() => void refresh(), POLL_MS)
    // Lives as long as the window does.
    listen(WINDOWS_CHANGED, () => void refresh()).catch(() => {})
    void refresh()
  }

  return {
    windows, here, open, cycleTick, cards, count, thumbs, thumbStatus, thumbsFailed,
    refresh, refreshThumbs, openOrCycle, start,
  }
})
