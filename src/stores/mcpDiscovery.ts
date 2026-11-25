import { defineStore } from 'pinia'
import { ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { invoke } from '@/lib/tauri'
import {
  DISCOVERY_CHANGED_EVENT,
  DISCOVERY_COMMANDS,
  viewKey,
  type DiscoveryImportOutcome,
  type DiscoveryView,
} from '@/lib/mcpDiscovery'
import { useMcpServersStore, type McpServer } from '@/stores/mcpServers'

/**
 * MCP servers found outside Puppeteer, one view per scope: Settings (no project,
 * global rows) and each open project's MCP tab (its rows + the global ones).
 * While a view is on screen (`watch` … `unwatch`) the backend's
 * `mcp_discovery:changed` (a scan, an import / sync, the engine probes starting
 * or ending) lists every watched scope again. Import, sync and the connector
 * switch change the MCP list too, so they refetch it.
 */
export const useMcpDiscoveryStore = defineStore('mcpDiscovery', () => {
  /** By `viewKey(projectId)`. */
  const views = ref<Record<string, DiscoveryView>>({})
  const loading = ref<Record<string, boolean>>({})
  const errors = ref<Record<string, string | null>>({})
  /** Row ids being imported and server ids being synced / switched. */
  const busy = ref(new Set<string>())

  /** Open views per scope key. */
  const watchers = new Map<string, number>()
  /** Latest call per scope key: a slower, older answer is dropped. */
  const tickets = new Map<string, number>()
  let unlisten: UnlistenFn | null = null
  let listening = false

  function viewOf(projectId: string | null): DiscoveryView | null {
    return views.value[viewKey(projectId)] ?? null
  }

  async function fetchView(projectId: string | null, fresh: boolean) {
    const key = viewKey(projectId)
    const mine = (tickets.get(key) ?? 0) + 1
    tickets.set(key, mine)
    loading.value = { ...loading.value, [key]: true }
    errors.value = { ...errors.value, [key]: null }
    try {
      const view = await invoke<DiscoveryView>(fresh ? DISCOVERY_COMMANDS.reload : DISCOVERY_COMMANDS.list, {
        projectId: projectId ?? null,
      })
      if (tickets.get(key) === mine) views.value = { ...views.value, [key]: view }
    } catch (e) {
      if (tickets.get(key) === mine) errors.value = { ...errors.value, [key]: String(e) }
    } finally {
      if (tickets.get(key) === mine) loading.value = { ...loading.value, [key]: false }
    }
  }

  /** The cached rows (the first call of the app scans the files). */
  function load(projectId: string | null) {
    return fetchView(projectId, false)
  }

  /** Read every source again; connector rows follow when the probes end (event). */
  function reload(projectId: string | null) {
    return fetchView(projectId, true)
  }

  function refetchWatched() {
    for (const key of watchers.keys()) void fetchView(key || null, false)
  }

  async function startListening() {
    if (listening) return
    listening = true
    const off = await listen(DISCOVERY_CHANGED_EVENT, refetchWatched).catch(() => null) // no event bus: reload by hand
    if (listening) unlisten = off
    else off?.()
  }

  /** A view of `projectId` is on screen: load it and follow changes. */
  function watch(projectId: string | null) {
    const key = viewKey(projectId)
    watchers.set(key, (watchers.get(key) ?? 0) + 1)
    void startListening()
    return load(projectId)
  }

  function unwatch(projectId: string | null) {
    const key = viewKey(projectId)
    const left = (watchers.get(key) ?? 1) - 1
    if (left > 0) watchers.set(key, left)
    else watchers.delete(key)
    if (watchers.size === 0) {
      listening = false
      unlisten?.()
      unlisten = null
    }
  }

  async function withBusy<T>(ids: string[], job: () => Promise<T>): Promise<T> {
    busy.value = new Set([...busy.value, ...ids])
    try {
      return await job()
    } finally {
      const next = new Set(busy.value)
      for (const id of ids) next.delete(id)
      busy.value = next
    }
  }

  /** Copy rows into the MCP list (off app-wide); one outcome per row id. */
  function importRows(rowIds: string[]): Promise<DiscoveryImportOutcome[]> {
    return withBusy(rowIds, async () => {
      const outcomes = await invoke<DiscoveryImportOutcome[]>(DISCOVERY_COMMANDS.importRows, { rowIds })
      await useMcpServersStore().fetchServers()
      return outcomes
    })
  }

  /** Make a managed copy equal its source again; its switches stay. */
  function sync(serverId: string): Promise<McpServer> {
    return withBusy([serverId], async () => {
      const server = await invoke<McpServer>(DISCOVERY_COMMANDS.sync, { serverId })
      await useMcpServersStore().fetchServers()
      return server
    })
  }

  /** App-wide switch of a claude.ai connector (the MCP form cannot save one). */
  function setConnectorEnabled(serverId: string, enabled: boolean): Promise<void> {
    return withBusy([serverId], async () => {
      await invoke(DISCOVERY_COMMANDS.setConnectorEnabled, { serverId, enabled })
      await useMcpServersStore().fetchServers()
      refetchWatched()
    })
  }

  return {
    views,
    loading,
    errors,
    busy,
    viewOf,
    load,
    reload,
    watch,
    unwatch,
    importRows,
    sync,
    setConnectorEnabled,
  }
})
