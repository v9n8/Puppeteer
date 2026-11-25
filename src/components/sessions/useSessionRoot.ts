// The folder the open Session's file viewer reads relative paths from: a
// worker's own worktree, else the Project. A worktree removed since (a cancelled
// or deleted worker's, cleaned up at startup) falls back to the Project, which
// the viewer read before workers had one; otherwise every file would fail.
import { computed, ref, watch, type ComputedRef } from 'vue'
import { sessionFileRoot } from '@/lib/sessionPaths'
import type { RunRecord } from '@/stores/runs'
import { useRunsStore } from '@/stores/runs'

export function useSessionRoot(
  run: ComputedRef<RunRecord | undefined>,
  projectPath: ComputedRef<string | undefined>,
): ComputedRef<string> {
  const runsStore = useRunsStore()
  // Worktrees the backend refused as a root. Until it answers, the worktree is
  // assumed there: a live worker's is, and its files show at once.
  const gone = ref(new Set<string>())
  watch(() => run.value?.worktree_path, (dir) => {
    if (!dir) return
    const mark = (isGone: boolean) => {
      if (gone.value.has(dir) === isGone) return
      const next = new Set(gone.value)
      if (isGone) next.add(dir)
      else next.delete(dir)
      gone.value = next
    }
    // An empty check still makes the backend resolve the root itself, so it
    // fails exactly when every read from this root would.
    runsStore.statPaths(dir, []).then(() => mark(false), () => mark(true))
  }, { immediate: true })

  return computed(() => {
    const r: { worktree_path?: string | null } = run.value ?? {}
    const exists = !r.worktree_path || !gone.value.has(r.worktree_path)
    return sessionFileRoot(r, projectPath.value, exists) ?? ''
  })
}
