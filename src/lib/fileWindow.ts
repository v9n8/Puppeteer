// Pops the file viewer out into a standalone OS window so a file can be read
// side-by-side with the main app (e.g. dragged onto a second monitor).
//
// The window loads the same SPA at `index.html?fileWindow=1&…`; App.vue detects
// the `fileWindow` flag and renders a bare, chrome-less FileViewer (no sidebar).
// One window per file (deduped by a deterministic label) — opening the same file
// again just focuses the existing window.
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { emit } from '@tauri-apps/api/event'
import { trackWindowForTray } from '@/lib/tray'
import { nextCascadePosition } from '@/lib/windowCascade'
import { popoutLabel, popoutUrl } from '@/lib/popout'
import { fileWindowQuery } from '@/lib/fileWindowCore'

/** Sent to an already-open file window: the run that opened it again. */
export const FILE_WINDOW_ORIGIN = 'file-window:origin'

export interface FileWindowOrigin {
  /** Window label (the file window's own `getCurrentWindow().label`). */
  label: string
  originRunId: string | null
}

// Small deterministic string hash (djb2) → safe, stable window label per file.
function hashLabel(s: string): string {
  let h = 5381
  for (let i = 0; i < s.length; i++) h = ((h << 5) + h + s.charCodeAt(i)) >>> 0
  return popoutLabel(`fileviewer-${h.toString(36)}`)
}

function basename(path: string): string {
  return path.split('/').pop() || path
}

/**
 * Open (or focus) a standalone window showing `path` from the project at
 * `projectPath`. `line`, when given, is highlighted/scrolled to. `originRunId`
 * is the run it was opened from: the window's Quote goes there (none = the
 * project's latest session).
 */
export async function openFileWindow(
  projectPath: string,
  path: string,
  line?: number | null,
  originRunId?: string | null,
): Promise<void> {
  if (!projectPath || !path) return
  const label = hashLabel(`${projectPath}|${path}`)

  // Already open for this file → bring it to front; whoever opened it last now
  // owns its Quote.
  const existing = await WebviewWindow.getByLabel(label)
  if (existing) {
    try {
      await existing.unminimize()
      await existing.show()
      await existing.setFocus()
      await emit(FILE_WINDOW_ORIGIN, { label, originRunId: originRunId ?? null } satisfies FileWindowOrigin)
    } catch { /* window may be mid-teardown */ }
    return
  }

  const query = fileWindowQuery({ projectPath, path, line: line ?? null, originRunId: originRunId ?? null })
  const win = new WebviewWindow(label, {
    url: popoutUrl(`index.html?${query}`),
    title: basename(path),
    width: 900,
    height: 820,
    minWidth: 420,
    minHeight: 300,
    // Opens focused, so start on top; the window lowers itself on blur unless
    // the user pins it (see useFloatingWindow).
    alwaysOnTop: true,
    ...(await nextCascadePosition()),
  })
  win.once('tauri://error', (e) => {
    console.error('[fileWindow] failed to open viewer window', e)
  })
  trackWindowForTray(win)
}
