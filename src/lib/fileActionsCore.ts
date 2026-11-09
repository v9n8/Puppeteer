// The opener choice behind lib/fileActions.ts's "Reveal in folder" and "Open in
// default app", free of Vue and Tauri so a test can pin which call runs.

/** The two `@tauri-apps/plugin-opener` calls these actions could reach for. */
export interface Opener {
  openPath(path: string): Promise<void>
  revealItemInDir(path: string): Promise<void>
}

/**
 * Select `abs` inside its folder, a folder included. Opening a folder instead
 * is not safe: on macOS a `.app` (or `.workflow`, `.prefPane`…) bundle is a
 * folder, and opening it launches it — no Gatekeeper prompt for a git clone.
 */
export function revealIn(opener: Opener, abs: string): Promise<void> {
  return opener.revealItemInDir(abs)
}

/**
 * "Open in default app". A folder is revealed, not opened, for the same reason
 * as above: its name does not tell the user that `Docs.app/` is an app.
 */
export function openInDefault(opener: Opener, abs: string, isDir: boolean): Promise<void> {
  return isDir ? opener.revealItemInDir(abs) : opener.openPath(abs)
}
