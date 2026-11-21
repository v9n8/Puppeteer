// Follows the `run:event:<id>` stream of the given running workers and exposes
// what each is doing (`lib/workerActivity`). A worker emits thousands of events
// a turn, so they land in a plain map and reach Vue at most once a second.
import { onScopeDispose, ref, watch, type Ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { nextActivity, type WorkerActivity } from '@/lib/workerActivity'

const FLUSH_MS = 1_000

export function useWorkerActivity(runningIds: Ref<string[]>) {
  const activity = ref<Record<string, WorkerActivity>>({})
  const latest = new Map<string, WorkerActivity>()
  const unlistens = new Map<string, Promise<UnlistenFn>>()
  let flushTimer: ReturnType<typeof setTimeout> | null = null

  function flush() {
    flushTimer = null
    activity.value = Object.fromEntries(latest)
  }

  function follow(id: string) {
    unlistens.set(
      id,
      listen<unknown>(`run:event:${id}`, (event) => {
        latest.set(id, nextActivity(latest.get(id), event.payload, Date.now()))
        flushTimer ??= setTimeout(flush, FLUSH_MS)
      }),
    )
  }

  function unfollow(id: string) {
    unlistens.get(id)?.then((off) => off()).catch(() => {})
    unlistens.delete(id)
    latest.delete(id)
  }

  watch(
    runningIds,
    (ids) => {
      const want = new Set(ids)
      for (const id of [...unlistens.keys()]) if (!want.has(id)) unfollow(id)
      for (const id of want) if (!unlistens.has(id)) follow(id)
      flush()
    },
    { immediate: true },
  )

  onScopeDispose(() => {
    if (flushTimer) clearTimeout(flushTimer)
    for (const id of [...unlistens.keys()]) unfollow(id)
  })

  return activity
}
