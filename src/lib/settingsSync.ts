// Keeps the shared settings store in step across windows: the Settings window
// saves, every other window (main, session pop-outs) re-reads.
// Pure on purpose: the Tauri `listen` / `emit` are passed in (see stores/appSettings.ts).

/** Broadcast to every window after a setting was saved. */
export const SETTINGS_CHANGED = 'settings:changed'

export interface SettingsChanged {
  keys: string[]
}

type Listen = (event: string, handler: (e: { payload: SettingsChanged }) => void) => Promise<unknown>
type Emit = (event: string, payload: SettingsChanged) => Promise<unknown>

/** Listen once per window; `refresh` is idempotent, so the emitting window re-reading its own save is harmless. */
export function startSettingsSync(listen: Listen, refresh: () => Promise<unknown>): Promise<unknown> {
  return listen(SETTINGS_CHANGED, () => {
    refresh().catch((e) => console.warn('[settingsSync] refresh failed', e))
  }).catch((e) => console.debug('[settingsSync] no event bridge', e))
}

/** Tell the other windows `keys` were saved. Best effort: the save itself already succeeded. */
export function notifySettingsChanged(emit: Emit, keys: string[]): Promise<unknown> {
  return emit(SETTINGS_CHANGED, { keys }).catch((e) => console.warn('[settingsSync] emit failed', e))
}
