// Where a file clicked in the Session area opens (V3-24): as a tab of the open
// Session. With no Session open there is no tab bar (the project's file tree
// still works then), so the file gets its own window as before.
import { openFileWindow } from '@/lib/fileWindow'
import { useSessionTabsStore } from '@/stores/sessionTabs'

export function openSessionFile(projectPath: string | undefined, runId: string | null, path: string, line: number | null) {
  if (!projectPath || !path) return
  if (runId) useSessionTabsStore().open(runId, path, line, projectPath)
  else void openFileWindow(projectPath, path, line, null)
}
