// Opens THE Secret notes window (views/SecretNotesWindow.vue, `?secretNotesWindow=1
// [&projectId=][&id=]`): the vault of secret notes, laid out like the Notes / Todos
// windows. There is only ever ONE such window (fixed label); opening it again
// focuses it and re-points it (project scope and / or the note to select).
//
// It has two homes (lib/windowPlacement keeps the choice, its own window by
// default): this window, or the `/secret-notes` screen of the main window
// (views/SecretNotesView.vue).
//
// The vault commands (src-tauri/src/commands/secret_vault.rs) refuse every other
// local webview but the main window. From a remote-desktop window it opens as `remote-secret-notes`,
// driving the controlled Puppeteer's vault through its bridge.
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { emit } from '@tauri-apps/api/event'
import { trackWindowForTray } from '@/lib/tray'
import { placementOptions, trackWindowBounds } from '@/lib/windowBounds'
import { SCREEN_OPEN_ROUTE, type ScreenOpenRoute, type ScreenTarget } from '@/lib/detachedScreens'
import { getOpenMode } from '@/lib/windowPlacement'
import type { Router } from 'vue-router'
import { popoutLabel, popoutUrl } from '@/lib/popout'
import { useFeaturesStore } from '@/stores/features'

/** Window label; the backend allows the vault commands from this window only. */
export const SECRET_NOTES_WINDOW_LABEL = 'secret-notes'

/** The screen's route in the main window (`?project=` and `?id=` carry what the window takes as options). */
export const SECRET_NOTES_PATH = '/secret-notes'

export const secretNotesTarget: ScreenTarget = {
  screen: 'secret-notes',
  path: SECRET_NOTES_PATH,
  openWindow: () => openSecretNotesWindow(),
}

/** Sent to the already-open window: re-point it. */
export const SECRET_NOTES_WINDOW_SELECT = 'secret-notes-window:select'

export interface SecretNotesWindowSelect {
  /** undefined = keep the current project scope. */
  projectId?: string | null
  /** Secret note to select. */
  id?: string | null
}

export interface OpenSecretNotesWindowOptions {
  /** Project to pre-filter the sidebar with (and default "+ New" to). */
  projectId?: string | null
  /** Secret note to select. */
  id?: string | null
}

/** Where the user last left the vault: the main window's screen, or its own window (the route guard decides the rest). */
export async function openSecretNotes(router: Router, opts: OpenSecretNotesWindowOptions = {}): Promise<void> {
  if (getOpenMode('secret-notes') !== 'main') {
    await openSecretNotesWindow(opts)
    return
  }
  await router.push(screenLocation(opts)).catch(() => {})
}

/** The same from a pop-out (no router there): the main window opens the screen. */
export async function openSecretNotesFromPopout(opts: OpenSecretNotesWindowOptions = {}): Promise<void> {
  if (getOpenMode('secret-notes') !== 'main') {
    await openSecretNotesWindow(opts)
    return
  }
  const { path, query } = screenLocation(opts)
  const qs = new URLSearchParams(query).toString()
  await emit<ScreenOpenRoute>(SCREEN_OPEN_ROUTE, { path: qs ? `${path}?${qs}` : path })
}

function screenLocation(opts: OpenSecretNotesWindowOptions): { path: string; query: Record<string, string> } {
  const query: Record<string, string> = {}
  if (opts.projectId) query.project = opts.projectId
  if (opts.id) query.id = opts.id
  return { path: SECRET_NOTES_PATH, query }
}

export async function openSecretNotesWindow(
  opts: OpenSecretNotesWindowOptions = {},
): Promise<WebviewWindow | null> {
  // Off: no window, whichever entry asked.
  if (!(await useFeaturesStore().isOnAfterLoad('security.secret_vault'))) return null
  const projectId = opts.projectId ?? null
  const label = popoutLabel(SECRET_NOTES_WINDOW_LABEL)

  const existing = await WebviewWindow.getByLabel(label)
  if (existing) {
    try {
      await existing.unminimize()
      await existing.show()
      await existing.setFocus()
      await emit(SECRET_NOTES_WINDOW_SELECT, {
        projectId: opts.id ? undefined : projectId,
        id: opts.id ?? null,
      } satisfies SecretNotesWindowSelect)
    } catch {
      /* window may be mid-teardown */
    }
    return existing
  }

  const params = new URLSearchParams({ secretNotesWindow: '1' })
  if (projectId) params.set('projectId', projectId)
  if (opts.id) params.set('id', opts.id)

  const win = new WebviewWindow(label, {
    url: popoutUrl(`index.html?${params.toString()}`),
    title: 'Secret notes — Puppeteer',
    minWidth: 760,
    minHeight: 480,
    // Where it was last left; the first time 1000 × 720 centred on the user's monitor.
    ...(await placementOptions('secret-notes', { width: 1000, height: 720, minWidth: 760, minHeight: 480 })),
  })
  win.once('tauri://error', (e) => {
    console.error('[secretNotesWindow] failed to open window', e)
  })
  trackWindowForTray(win)
  trackWindowBounds(win, 'secret-notes')
  return win
}
