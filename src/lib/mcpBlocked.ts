// MCP servers Puppeteer does not manage, blocked in a session (backend
// `run:mcp_blocked:{id}`): the sidecar denied a call to one of its tools, or the
// engine still started it. The session shows them in a small notice; these pure
// helpers keep the list it reads. Free of Vue / `@/` imports so they can be
// unit-tested with `node --test`.

export type McpBlockedReason = 'call' | 'loaded'

export interface McpBlockedEntry {
  /** Server name as the other tool's config wrote it (untrusted text). */
  server: string
  /** Where the engine says it came from: `claudeai`, `user`, `project`, `plugin`… */
  source: string
  /** Denied calls to its tools; 0 when it was only seen loaded. */
  calls: number
}

/** The backend's report as the store keeps it; null when unusable. */
export function readMcpBlocked(raw: unknown): { server: string; source: string; reason: McpBlockedReason } | null {
  if (!raw || typeof raw !== 'object') return null
  const o = raw as Record<string, unknown>
  if (typeof o.server !== 'string' || !o.server) return null
  return {
    server: o.server,
    source: typeof o.source === 'string' && o.source ? o.source : 'unknown',
    reason: o.reason === 'loaded' ? 'loaded' : 'call',
  }
}

/** `list` with one more report: one entry per server and source, in the order
 *  first seen, counting the denied calls. */
export function addMcpBlocked(list: McpBlockedEntry[], raw: unknown): McpBlockedEntry[] {
  const report = readMcpBlocked(raw)
  if (!report) return list
  const call = report.reason === 'call' ? 1 : 0
  const at = list.findIndex((e) => e.server === report.server && e.source === report.source)
  if (at < 0) return [...list, { server: report.server, source: report.source, calls: call }]
  return list.map((e, i) => (i === at ? { ...e, calls: e.calls + call } : e))
}
