// File Transfer: copy files and folders between this machine and the Puppeteer a
// remote-desktop window controls. It has two homes (lib/windowPlacement keeps the
// choice, its own window by default): its own window (views/FileTransferWindow.vue,
// `?fileTransferWindow=1`, label `remote-file-transfer`, talking to the controlled
// Puppeteer through the remote-desktop window, lib/ipc/muxTransport.ts), or the
// `/file-transfer` screen inside the remote-desktop window itself. Only offered
// from a remote-desktop window. Opening the window again focuses it.
import { WebviewWindow } from '@tauri-apps/api/webviewWindow'
import { trackWindowForTray } from '@/lib/tray'
import { placementOptions, trackWindowBounds } from '@/lib/windowBounds'
import type { ScreenTarget } from '@/lib/detachedScreens'
import { getOpenMode } from '@/lib/windowPlacement'
import type { Router } from 'vue-router'
import { isRemoteMode } from '@/lib/ipc/remote'
import { popoutLabel, popoutUrl } from '@/lib/popout'
import { awaitWindowCreated } from '@/lib/windowCreateGuard'

/** Local label (the window itself is `remote-file-transfer`, see lib/popout.ts). */
const FILE_TRANSFER_WINDOW_LABEL = 'file-transfer'

/** File transfer is between two machines: only offered while controlling another Puppeteer. */
export function canOpenFileTransfer(): boolean {
  return isRemoteMode()
}

/** The screen's route in the remote-desktop window, and how to open its own window. */
export const FILE_TRANSFER_PATH = '/file-transfer'

export const fileTransferTarget: ScreenTarget = {
  screen: 'file-transfer',
  path: FILE_TRANSFER_PATH,
  openWindow: () => openFileTransferWindow(),
}

/** "Files" in the device popover: where the user last left File Transfer (the route guard decides). */
export async function openFileTransfer(router: Router): Promise<void> {
  if (!canOpenFileTransfer()) return
  if (getOpenMode('file-transfer') === 'main') {
    await router.push(FILE_TRANSFER_PATH)
    return
  }
  await openFileTransferWindow()
}

// In-flight creation guard (like lib/remoteDesktopWindow.ts).
let creating: Promise<WebviewWindow> | null = null

async function focusWindow(win: WebviewWindow) {
  try {
    await win.unminimize()
    await win.show()
    await win.setFocus()
  } catch {
    /* window may be mid-teardown */
  }
}

export async function openFileTransferWindow(): Promise<WebviewWindow | null> {
  if (!canOpenFileTransfer()) return null
  if (creating) {
    const win = await creating
    await focusWindow(win)
    return win
  }
  const label = popoutLabel(FILE_TRANSFER_WINDOW_LABEL)
  const existing = await WebviewWindow.getByLabel(label)
  if (existing) {
    await focusWindow(existing)
    return existing
  }
  if (creating) return creating
  creating = createFileTransferWindow(label)
  try {
    return await creating
  } finally {
    creating = null
  }
}

async function createFileTransferWindow(label: string): Promise<WebviewWindow> {
  const win = new WebviewWindow(label, {
    url: popoutUrl('index.html?fileTransferWindow=1'),
    title: 'File Transfer — Puppeteer',
    minWidth: 820,
    minHeight: 520,
    // Where it was last left; the first time 1100 × 760 centred on the current monitor.
    ...(await placementOptions('file-transfer', { width: 1100, height: 760, minWidth: 820, minHeight: 520 })),
  })
  await awaitWindowCreated(win, 'fileTransferWindow')
  trackWindowForTray(win)
  trackWindowBounds(win, 'file-transfer')
  return win
}
