/**
 * The terminal apps "Open in terminal" can use on each OS, for the Settings picker.
 * Mirrors `terminal_apps` in `src-tauri/src/commands/projects.rs`, which also turns a
 * value saved on another OS into this OS's default.
 */

export type DesktopOs = 'macos' | 'windows' | 'linux'

/** The OS the app runs on, from the webview's user agent. */
export function desktopOs(userAgent: string): DesktopOs {
  if (/mac/i.test(userAgent)) return 'macos'
  if (/windows/i.test(userAgent)) return 'windows'
  return 'linux'
}

/** `terminal_app` values and their label keys (`settings.general.*`), the OS default first. */
export const TERMINAL_APPS: Record<DesktopOs, readonly { value: string; labelKey: string }[]> = {
  macos: [
    { value: 'terminal', labelKey: 'terminalDefault' },
    { value: 'iterm', labelKey: 'terminalIterm' },
  ],
  windows: [
    { value: 'powershell', labelKey: 'terminalPowershell' },
    { value: 'wt', labelKey: 'terminalWindowsTerminal' },
    { value: 'cmd', labelKey: 'terminalCmd' },
  ],
  linux: [
    { value: 'auto', labelKey: 'terminalAuto' },
    { value: 'gnome-terminal', labelKey: 'terminalGnome' },
    { value: 'konsole', labelKey: 'terminalKonsole' },
    { value: 'xfce4-terminal', labelKey: 'terminalXfce' },
    { value: 'xterm', labelKey: 'terminalXterm' },
  ],
}
