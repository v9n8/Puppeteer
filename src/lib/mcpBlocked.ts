// MCP servers Puppeteer does not manage, blocked in a session (backend
// `run:mcp_blocked:{id}`): the sidecar denied a call to one of its tools, or the
// engine still started it. The same event names a server Puppeteer meant to
// pass but left out at start (`skipped`, e.g. a claude.ai connector whose
// profile's last check did not report it). The session shows them in a small notice; these pure
// helpers keep the list it reads. Free of Vue / `@/` imports so they can be
// unit-tested with `node --test`.

export type McpBlockedReason = 'call' | 'no_provenance' | 'loaded' | 'skipped'

export interface McpBlockedEntry {
  /** Server name as the other tool's config wrote it (untrusted text). */
  server: string
  /** Where the engine says it came from: `claudeai`, `user`, `project`, `plugin`… */
  source: string
  /** Denied calls to its tools; 0 when it was only seen loaded. */
  calls: number
  /** A server Puppeteer manages, left out of the session at start. */
  skipped?: true
  /** Its calls were denied because the Claude CLI did not say where the server
   *  came from (an older CLI blocks every MCP tool, Puppeteer's own included). */
  noProvenance?: true
}

/** The backend's report as the store keeps it; null when unusable. */
export function readMcpBlocked(raw: unknown): { server: string; source: string; reason: McpBlockedReason } | null {
  if (!raw || typeof raw !== 'object') return null
  const o = raw as Record<string, unknown>
  if (typeof o.server !== 'string' || !o.server) return null
  return {
    server: o.server,
    source: typeof o.source === 'string' && o.source ? o.source : 'unknown',
    reason: o.reason === 'loaded' || o.reason === 'skipped' || o.reason === 'no_provenance' ? o.reason : 'call',
  }
}

/** `list` with one more report: one entry per server and source, in the order
 *  first seen, counting the denied calls. */
export function addMcpBlocked(list: McpBlockedEntry[], raw: unknown): McpBlockedEntry[] {
  const report = readMcpBlocked(raw)
  if (!report) return list
  const noProvenance = report.reason === 'no_provenance'
  const call = report.reason === 'call' || noProvenance ? 1 : 0
  const at = list.findIndex((e) => e.server === report.server && e.source === report.source)
  if (at < 0) {
    const entry: McpBlockedEntry = { server: report.server, source: report.source, calls: call }
    if (report.reason === 'skipped') entry.skipped = true
    if (noProvenance) entry.noProvenance = true
    return [...list, entry]
  }
  return list.map((e, i) => (i === at ? { ...e, calls: e.calls + call, ...(noProvenance ? { noProvenance: true as const } : {}) } : e))
}

/** `list` with every report of `raw`. Anything but an array leaves `list` as it
 *  is; a report seen already is not added twice. */
export function addMcpBlockedAll(list: McpBlockedEntry[], raw: unknown): McpBlockedEntry[] {
  return Array.isArray(raw) ? raw.reduce<McpBlockedEntry[]>(addMcpBlocked, list) : list
}

/** The servers one start of a run left out (backend `run:mcp_skipped:{id}` and
 *  `list_run_mcp_skipped`); `start` grows with every start, 0 = none known.
 *  Null when unusable. */
export function readMcpSkipped(raw: unknown): { start: number; servers: unknown[] } | null {
  if (!raw || typeof raw !== 'object') return null
  const o = raw as Record<string, unknown>
  if (typeof o.start !== 'number' || !Array.isArray(o.servers)) return null
  return { start: o.start, servers: o.servers }
}

/** `list` with its left-out servers replaced by `servers`: a later start may pass
 *  a server an earlier one left out. Only `skipped` reports are taken. */
export function replaceMcpSkipped(list: McpBlockedEntry[], servers: unknown[]): McpBlockedEntry[] {
  const skipped = servers.filter((s) => readMcpBlocked(s)?.reason === 'skipped')
  return addMcpBlockedAll(list.filter((e) => !e.skipped), skipped)
}
