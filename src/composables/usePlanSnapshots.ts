// Full plan-usage snapshots by row key (`claude`, `claude:<id>`, `codex`, `codex:<id>`):
// shared by the usage rings on the status bar and Settings › Limits. Reads are
// local DB reads (no engine call).
import { ref } from 'vue'
import { invoke } from '@/lib/tauri'
import { planUsageRead, type PlanUsageSnapshot } from '@/lib/usageRing'
import { createLatestRead } from '@/lib/latestRead'

export function usePlanSnapshots() {
  const snapshots = ref<Record<string, PlanUsageSnapshot | null>>({})
  const latest = createLatestRead()

  /** Keeps the old value on error; a read overtaken by a newer one of the same key is dropped (RT-15). */
  async function loadSnapshot(key: string) {
    const current = latest.begin(key)
    try {
      const read = planUsageRead(key)
      const snapshot = await invoke<PlanUsageSnapshot | null>(read.cmd, read.args)
      if (current()) snapshots.value[key] = snapshot
    } catch {
      // leave the previous value on transient errors
    }
  }

  async function loadSnapshots(keys: string[]) {
    await Promise.all(keys.map(loadSnapshot))
  }

  return { snapshots, loadSnapshot, loadSnapshots }
}
