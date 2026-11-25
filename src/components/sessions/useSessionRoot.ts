// The folder the open Session's file viewer reads relative paths from: a
// worker's own worktree, else the Project. A worktree this caller can't read
// from (removed since, as a cancelled or deleted worker's is at startup, or out
// of a remote desktop's reach) falls back to the Project, which the viewer read
// before workers had one; otherwise every file would fail.
import { computed, ref, watch, type ComputedRef } from 'vue'
import { WORKTREE_PROBE, sessionFileRoot, worktreeReadable } from '@/lib/sessionPaths'
import type { RunRecord } from '@/stores/runs'
import { useRunsStore } from '@/stores/runs'

export function useSessionRoot(
  run: ComputedRef<RunRecord | undefined>,
  projectPath: ComputedRef<string | undefined>,
): ComputedRef<string> {
  const runsStore = useRunsStore()
  // Worktrees the probe found unreadable. Until it answers, the worktree is
  // assumed readable: a live worker's is, and its files show at once.
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
    // A failed call (e.g. the root no longer resolves) answers "not readable".
    runsStore.statPaths(dir, [WORKTREE_PROBE])
      .then((answer) => answer, () => null)
      .then((answer) => mark(!worktreeReadable(answer)))
  }, { immediate: true })

  return computed(() => {
    const r: { worktree_path?: string | null } = run.value ?? {}
    const exists = !r.worktree_path || !gone.value.has(r.worktree_path)
    return sessionFileRoot(r, projectPath.value, exists) ?? ''
  })
}
