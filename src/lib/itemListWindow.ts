// Opens the Notes window (views/ItemListWindow.vue, `?itemListWindow=1&kind=note
// [&projectId=][&id=]`): a master–detail window laid out like the Slack threads
// window, a full-height sidebar of items, the selected one read and edited inline
// on the right. There is at most ONE (fixed label); opening it again focuses it and
// re-points it (project scope and / or the item to select).
//
// Todos no longer have a window of their own: `'todo'` opens the Tasks screen in
// its list view (lib/focusWindow `openTasks`), in the main window or its own.
//
// Every "open this note / todo" in the app lands here with `id`.
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { emit } from '@tauri-apps/api/event'
import { trackWindowForTray } from '@/lib/tray'
import { placementOptions, trackWindowBounds } from '@/lib/windowBounds'
import { SCREEN_OPEN_ROUTE, type ScreenOpenRoute, type ScreenTarget } from '@/lib/detachedScreens'
import { getOpenMode } from '@/lib/windowPlacement'
import { NOTES_PATH, notesPath } from '@/lib/screenRoutes'
import type { ItemKind } from '@/lib/itemWindow'
import { openSlackWindow } from '@/lib/slackWindow'
import { openTasks } from '@/lib/focusWindow'
import { popoutLabel, popoutUrl } from '@/lib/popout'
import { useFeaturesStore } from '@/stores/features'

/**
 * The list windows' kinds: todos and notes. Slack threads have their own window
 * (lib/slackWindow); `'slack'` here is only accepted to redirect there.
 */
export type ItemListKind = ItemKind | 'slack'

/**
 * Notes has two homes (lib/windowPlacement keeps the choice, the main window by
 * default): the `/notes` screen of the main window, or this window.
 */
export const notesTarget: ScreenTarget = {
  screen: 'notes',
  path: NOTES_PATH,
  openWindow: () => openNotesWindow(),
}

/** The fixed window label of a kind's list window. */
export function itemListWindowLabel(kind: ItemKind): string {
  return popoutLabel(`item-list-${kind}`)
}

/**
 * Sent to an already-open list window to re-point it (project scope and / or
 * the item to select). Scoped per kind, so re-opening the todo window never
 * reaches the note window and vice versa.
 */
export function itemListSelectEvent(kind: ItemKind): string {
  return `item-list:select:${kind}`
}

export interface ItemListSelect {
  /** undefined = keep the current project scope. */
  projectId?: string | null
  /** Item to select. */
  id?: string | null
}

export interface OpenItemListOptions {
  /** Project to pre-filter the sidebar with (and default "+ New" to). */
  projectId?: string | null
  /** Item to select. */
  id?: string | null
}

/**
 * Open `kind` where the user keeps it: todos in the Tasks screen, notes on the
 * `/notes` screen of the main window or in the Notes window, optionally scoped to
 * a project and / or with an item selected.
 */
export async function openItemListWindow(
  kind: ItemListKind = 'todo',
  opts: OpenItemListOptions = {},
): Promise<WebviewWindow | null> {
  const projectId = opts.projectId ?? null
  // Slack threads moved out of this list into their own window.
  if (kind === 'slack') return openSlackWindow({ projectId, id: opts.id ?? undefined })
  // Todos moved into the Tasks screen (list view), where the user keeps it.
  if (kind === 'todo') {
    await openTasks({ view: 'list', projectId: opts.id ? null : projectId, id: opts.id })
    return null
  }
  if (getOpenMode('notes') === 'window') return openNotesWindow({ projectId, id: opts.id })
  // Off: nothing opens, whichever entry asked.
  if (!(await useFeaturesStore().isOnAfterLoad('productivity.notes'))) return null
  await emit<ScreenOpenRoute>(SCREEN_OPEN_ROUTE, { path: notesPath({ projectId, id: opts.id }) })
  return null
}

/** Open (or focus) THE Notes window, whatever the saved choice says (Open in new window, the route guard). */
export async function openNotesWindow(opts: OpenItemListOptions = {}): Promise<WebviewWindow | null> {
  const projectId = opts.projectId ?? null
  const kind: ItemKind = 'note'
  // Off: no window, whichever entry asked.
  if (!(await useFeaturesStore().isOnAfterLoad('productivity.notes'))) return null

  const label = itemListWindowLabel(kind)
  const existing = await WebviewWindow.getByLabel(label)
  if (existing) {
    try {
      await existing.unminimize()
      await existing.show()
      await existing.setFocus()
      // Re-point the already-open window. Selecting an item leaves the
      // project scope alone (the window widens its filter if it must).
      await emit(itemListSelectEvent(kind), {
        projectId: opts.id ? undefined : projectId,
        id: opts.id ?? null,
      } satisfies ItemListSelect)
    } catch {
      /* window may be mid-teardown */
    }
    return existing
  }

  const params = new URLSearchParams({ itemListWindow: '1', kind })
  if (projectId) params.set('projectId', projectId)
  if (opts.id) params.set('id', opts.id)

  const win = new WebviewWindow(label, {
    url: popoutUrl(`index.html?${params.toString()}`),
    title: 'Notes — Puppeteer',
    minWidth: 760,
    minHeight: 480,
    // Where it was last left; the first time 1000 × 720 centred on the user's monitor.
    ...(await placementOptions('notes', { width: 1000, height: 720, minWidth: 760, minHeight: 480 })),
  })
  win.once('tauri://error', (e) => {
    console.error('[itemListWindow] failed to open window', e)
  })
  trackWindowForTray(win)
  trackWindowBounds(win, 'notes')
  return win
}
