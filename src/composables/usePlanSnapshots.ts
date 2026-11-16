// Full plan-usage snapshots by row key (`claude`, `claude:<id>`, `codex`, `codex:<id>`):
// shared by the usage rings on the status bar and Settings › Limits. Reads are
// local DB reads (no engine call).
import { ref } from 'vue'
import { invoke } from '@/lib/tauri'
import { planUsageRead, type PlanUsageSnapshot } from '@/lib/usageRing'

export function usePlanSnapshots() {
  const snapshots = ref<Record<string, PlanUsageSnapshot | null>>({})

  /** Keeps the old value on error. */
  async function loadSnapshot(key: string) {
    try {
      const read = planUsageRead(key)
      snapshots.value[key] = await invoke<PlanUsageSnapshot | null>(read.cmd, read.args)
    } catch {
      // leave the previous value on transient errors
    }
  }

  async function loadSnapshots(keys: string[]) {
    await Promise.all(keys.map(loadSnapshot))
  }

  return { snapshots, loadSnapshot, loadSnapshots }
}
