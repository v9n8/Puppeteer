import { defineStore } from 'pinia'
import { reactive } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { addMcpBlocked, type McpBlockedEntry } from '@/lib/mcpBlocked'

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

  function listenTo(runId: string): Promise<UnlistenFn> {
    return listen<unknown>(`run:mcp_blocked:${runId}`, (event) => {
      byRun.set(runId, addMcpBlocked(byRun.get(runId) ?? [], event.payload))
    })
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
