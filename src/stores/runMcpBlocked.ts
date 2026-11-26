import { defineStore } from 'pinia'
import { reactive } from 'vue'
import { invoke } from '@/lib/tauri'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { addMcpBlocked, readMcpSkipped, replaceMcpSkipped, type McpBlockedEntry } from '@/lib/mcpBlocked'

/**
 * MCP servers Puppeteer does not manage, blocked in each session (backend
 * `run:mcp_blocked:{id}`), and the servers its last start left out
 * (`run:mcp_skipped:{id}`), for the session's notice. Kept per run after the run
 * ends — the notice shows whatever the run status — until the user dismisses it
 * or the live-runs store drops the session (deleted run, evicted finished one).
 * The live-runs store attaches the listener with the run's others, so a block
 * in a session that is not on screen is not missed.
 */
export const useRunMcpBlockedStore = defineStore('runMcpBlocked', () => {
  const byRun = reactive(new Map<string, McpBlockedEntry[]>())
  /** The newest start whose left-out list each run took. Kept on dismiss, so the
   *  same list read again does not bring a dismissed notice back. */
  const skippedStart = new Map<string, number>()

  /** Take one start's left-out list if it is newer than the one shown. */
  function applySkipped(runId: string, raw: unknown) {
    const skipped = readMcpSkipped(raw)
    if (!skipped || skipped.start <= (skippedStart.get(runId) ?? 0)) return
    skippedStart.set(runId, skipped.start)
    const next = replaceMcpSkipped(byRun.get(runId) ?? [], skipped.servers)
    if (next.length) byRun.set(runId, next)
    else byRun.delete(runId)
  }

  /**
   * Read the servers `runId`'s last start left out (the backend keeps them until
   * the run is deleted): for a listener that attached after the start sent them
   * (conductor worker, schedule, Remote start), or a session shown without a live
   * listener (its run ended before the view attached, or the window reloaded).
   */
  function readSkipped(runId: string) {
    invoke<unknown>('list_run_mcp_skipped', { runId })
      .then((raw) => applySkipped(runId, raw))
      .catch((e) => console.warn('[runMcpBlocked] could not read the servers left out at start', e))
  }

  async function listenTo(runId: string): Promise<UnlistenFn> {
    const unlistenBlocked = await listen<unknown>(`run:mcp_blocked:${runId}`, (event) => {
      byRun.set(runId, addMcpBlocked(byRun.get(runId) ?? [], event.payload))
    })
    const unlistenSkipped = await listen<unknown>(`run:mcp_skipped:${runId}`, (event) => {
      applySkipped(runId, event.payload)
    })
    // Not awaited — the run's other listeners must not wait for it.
    readSkipped(runId)
    return () => {
      unlistenBlocked()
      unlistenSkipped()
    }
  }

  function blockedOf(runId: string | null | undefined): McpBlockedEntry[] {
    return (runId && byRun.get(runId)) || []
  }

  /** Hide the notice; a later block, or a later start's left-out list, shows it again. */
  function dismiss(runId: string) {
    byRun.delete(runId)
  }

  /** The session is dropped: forget everything of the run, so reopening it reads its list again. */
  function forget(runId: string) {
    byRun.delete(runId)
    skippedStart.delete(runId)
  }

  return { listenTo, readSkipped, blockedOf, dismiss, forget }
})
