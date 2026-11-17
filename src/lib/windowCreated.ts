// `new WebviewWindow(...)` returns at once; a failed creation (bad label, no
// permission, …) arrives later as `tauri://error`. Await this to learn the outcome.
interface CreationEvents {
  once: (event: string, handler: (e: { payload?: unknown }) => void) => unknown
}

export function whenWindowCreated(win: CreationEvents): Promise<void> {
  return new Promise((resolve, reject) => {
    void win.once('tauri://created', () => resolve())
    void win.once('tauri://error', (e) => {
      const p = e?.payload
      reject(new Error(typeof p === 'string' ? p : 'window creation failed'))
    })
  })
}
