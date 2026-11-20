// "Re-sync from transcript" (RT-16, #19): rebuild a session's log from the engine's
// transcript (`resync_run`). Shared by the History / header menu and the Board card.
import { invoke } from '@/lib/tauri'

/** The confirm dialog: the rebuild drops lines only the app kept and may move bookmarks / quotes. */
export function resyncConfirmOptions(t: (key: string) => string) {
  return {
    title: t('run.resync.confirmTitle'),
    message: t('run.resync.confirmMessage'),
    confirmLabel: t('run.resync.confirm'),
  }
}

/** Rebuild `runId`'s log; resolves to the transcript it was rebuilt from. */
export async function resyncRun(runId: string): Promise<string> {
  const out = await invoke<{ transcript_path: string }>('resync_run', { runId })
  return out.transcript_path
}
