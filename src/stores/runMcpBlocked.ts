import { defineStore } from 'pinia'
import { reactive } from 'vue'
import { invoke } from '@/lib/tauri'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { addMcpBlocked, addMcpBlockedAll, type McpBlockedEntry } from '@/lib/mcpBlocked'

/**
 * MCP servers Puppeteer does not manage, blocked in each session (backend
 * `run:mcp_blocked:{id}`), for the session's notice. Kept per run after the run
 * ends — the notice shows whatever the run status — until the user dismisses it
 * or the live-runs store drops the session (deleted run, evicted finished one).
 * The live-runs store attaches the listener with the run's others, so a block
 * in a session that is not on screen is not missed.
 */
export const useRunMcpBlockedStore = defineStore('runMcpBlocked', () => {
  const byRun = reactive(new Map<string, McpBlockedEntry[]>())

  async function listenTo(runId: string): Promise<UnlistenFn> {
    const unlisten = await listen<unknown>(`run:mcp_blocked:${runId}`, (event) => {
      byRun.set(runId, addMcpBlocked(byRun.get(runId) ?? [], event.payload))
    })
    // The start reports the servers it left out before the sidecar runs, so a
    // session started outside the composer (conductor worker, schedule, Remote)
    // attaches after that: read them back. Not awaited — the run's other
    // listeners must not wait for it.
    invoke<unknown>('list_run_mcp_skipped', { runId })
      .then((reports) => {
        const next = addMcpBlockedAll(byRun.get(runId) ?? [], reports)
        if (next.length) byRun.set(runId, next)
      })
      .catch((e) => console.warn('[runMcpBlocked] could not read the servers left out at start', e))
    return unlisten
  }

  function blockedOf(runId: string | null | undefined): McpBlockedEntry[] {
    return (runId && byRun.get(runId)) || []
  }

  /** Hide the notice; a later block shows it again. */
  function dismiss(runId: string) {
    byRun.delete(runId)
  }

  return { listenTo, blockedOf, dismiss }
})
