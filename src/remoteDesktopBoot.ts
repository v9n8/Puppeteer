/**
 * Boot of a remote-desktop window (`?remote=1`).
 *
 * The main one (`remote-desktop`, see lib/remoteDesktopWindow.ts): without a
 * stored sign-in it shows the password login; with one it connects the IPC shim
 * to the controlled Puppeteer FIRST (any invoke/listen made before that would hit
 * this machine's backend), then mounts the normal app (its sidebar shows the
 * link: components/remote/RemoteDesktopIndicator.vue) and serves that one
 * connection to its pop-outs (lib/ipc/mux.ts).
 *
 * A pop-out opened from a remote window (`remote-…` label, lib/ipc/windows.ts)
 * has no connection of its own: its shim talks to the controlled Puppeteer through
 * the main remote-desktop window.
 */
import { createApp, type App } from 'vue'
import { getAllWebviewWindows } from '@tauri-apps/api/webviewWindow'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-real/core'
import { applyThemeMode } from './lib/themeMode'
import { applyAppearance } from './lib/appearanceSync'
import { i18n } from './i18n'
import { installRemoteIpc, startRemoteIpc } from './lib/ipc/remote'
import { startMuxClient, startMuxHost } from './lib/ipc/muxTransport'
import { broadcastResync, installResyncReload } from './lib/ipc/resync'
import { REMOTE_OWNER_LABEL, isRemoteLabel } from './lib/ipc/windows'
import { installRemoteDiagnostics } from './lib/ipc/diagnostics'
import { loadRemoteDesktopSession, saveRemoteDesktopSession } from './lib/remoteDesktopSession'
import RemoteDesktopLogin from './components/remote/RemoteDesktopLogin.vue'

/** Signed out: pop-outs of the previous sign-in have nothing to talk to. */
async function closeRemotePopouts(): Promise<void> {
  const wins = await getAllWebviewWindows().catch(() => [])
  for (const w of wins) {
    if (isRemoteLabel(w.label) && w.label !== REMOTE_OWNER_LABEL) await w.close().catch(() => {})
  }
}

/**
 * The login window mounts without App.vue, so it reads the look itself (before
 * mounting: no light flash). A failure leaves the system look and never blocks sign-in.
 */
async function applyStoredAppearance(): Promise<void> {
  const media = window.matchMedia('(prefers-color-scheme: dark)')
  let settings: { theme?: string; color_theme?: string } = {}
  try {
    settings = await invoke<{ theme?: string; color_theme?: string }>('get_settings')
  } catch (e) {
    console.error('[remoteDesktop] get_settings failed', e)
  }
  applyThemeMode(settings.theme ?? 'system', document.documentElement, media)
  applyAppearance({ colorTheme: settings.color_theme }, document.documentElement, media.matches)
}

export function bootRemoteDesktop(mountApp: () => App): void | Promise<void> {
  const label = getCurrentWindow().label
  if (label !== REMOTE_OWNER_LABEL) {
    // Calls are queued until the main remote-desktop window answers.
    installRemoteIpc(startMuxClient(label))
    installRemoteDiagnostics()
    void installResyncReload().catch(() => {})
    mountApp()
    return
  }

  const session = loadRemoteDesktopSession()
  if (!session) {
    void closeRemotePopouts()
    return applyStoredAppearance().then(() => {
      createApp(RemoteDesktopLogin).use(i18n).mount('#app')
    })
  }

  // Calls made from here on are queued until the session authenticates.
  const client = startRemoteIpc(
    session.link,
    session.token,
    (token) => saveRemoteDesktopSession({ ...session, token }),
    broadcastResync,
  )
  installRemoteDiagnostics()
  void installResyncReload().catch(() => {})
  void startMuxHost(client).catch((e) => console.error('[remoteDesktop] pop-out link failed', e))
  mountApp()
}
