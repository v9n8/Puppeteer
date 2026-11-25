// The folder the open Session's file viewer reads relative paths from: a
// worker's own worktree, else the Project. A worktree this caller can't read
// from (removed since, as a cancelled or deleted worker's is at startup, or out
// of a remote desktop's reach) falls back to the Project, which the viewer read
// before workers had one; otherwise every file would fail.
import { computed, ref, watch, type ComputedRef } from 'vue'
import { WORKTREE_PROBE, sessionRootState, worktreeReadable } from '@/lib/sessionPaths'
import type { RunRecord } from '@/stores/runs'
import { useRunsStore } from '@/stores/runs'

export function useSessionRoot(
  run: ComputedRef<RunRecord | undefined>,
  projectPath: ComputedRef<string | undefined>,
  status: ComputedRef<string>,
): { root: ComputedRef<string>; settled: ComputedRef<boolean> } {
  const runsStore = useRunsStore()
  // The probe's last answer per worktree. Until the first one, the worktree is
  // used (a live worker's is readable, and its files show at once) but not settled.
  const readable = ref(new Map<string, boolean>())
  // Asked again on every status change too: resuming a cancelled worker brings
  // its swept worktree back at the same path.
  watch([() => run.value?.worktree_path, status], ([dir]) => {
    if (!dir) return
    const mark = (ok: boolean) => {
      if (readable.value.get(dir) === ok) return
      readable.value = new Map(readable.value).set(dir, ok)
    }
    // A failed call (e.g. the root no longer resolves) answers "not readable".
    runsStore.statPaths(dir, [WORKTREE_PROBE])
      .then((answer) => answer, () => null)
      .then((answer) => mark(worktreeReadable(answer)))
  }, { immediate: true })

  const state = computed(() => {
    const r: { worktree_path?: string | null } = run.value ?? {}
    return sessionRootState(r, projectPath.value, r.worktree_path ? readable.value.get(r.worktree_path) : undefined)
  })
  return {
    root: computed(() => state.value.root ?? ''),
    settled: computed(() => state.value.settled),
  }
}
