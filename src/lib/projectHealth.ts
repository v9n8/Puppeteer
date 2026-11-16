// Pure helpers behind the Configuration health card of a Project's Overview
// (V3-48): which MCP servers get a connection test, and the rows the card draws
// from what each source (repos, account, MCP, conflicts, servers) really returned.
import type { ProjectMcpServer } from '@/stores/mcpServers'
import type { ProjectSectionId } from '@/lib/projectSections'
import type { ProjectSyncConflict } from '@/stores/projects'

/** One data source: still loading, failed with the real error, or loaded. */
export type Source<T> =
  | { state: 'loading' }
  | { state: 'error'; error: string }
  | { state: 'ready'; value: T }

export interface McpFailure { name: string; message: string; at: number }

export interface McpHealth {
  /** Servers on for the project (what "MCP: N on" counts). */
  enabled: number
  /** Tests still to run (or the list still loading). */
  checking: boolean
  failures: McpFailure[]
}

export interface HealthLine {
  key?: string
  params?: Record<string, string | number>
  /** Shown as is: names, or a message a command returned. */
  text?: string
}

export type HealthLink = { target: ProjectSectionId | 'repos'; labelKey: string }

export interface HealthRow {
  key: string
  tone: 'ok' | 'warn' | 'busy'
  title: { key: string; params?: Record<string, string | number> }
  subs: HealthLine[]
  link?: HealthLink
  action?: 'recheck'
}

export interface HealthInput {
  /** `owner/repo` (or path) of each repository. */
  repos: Source<string[]>
  engine: 'claude' | 'codex'
  /** Label of the account new sessions use; null when there is none. */
  account: Source<string | null>
  mcp: Source<McpHealth>
  conflicts: Pick<ProjectSyncConflict, 'id' | 'name' | 'kind'>[]
  /** Label of each server enabled for the project. */
  servers: Source<string[]>
}

const SEP = ' · '

/** Servers worth a connection test: on app-wide and on for this project. */
export function mcpToTest<T extends Pick<ProjectMcpServer, 'enabled' | 'enabled_for_project'>>(servers: readonly T[]): T[] {
  return servers.filter((s) => s.enabled && s.enabled_for_project)
}

/** "09:42" from milliseconds since the epoch, in local time. */
export function clockText(ms: number): string {
  const d = new Date(ms)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

const loadError = (what: string, error: string): HealthRow => ({
  key: what,
  tone: 'warn',
  title: { key: `projectDetail.health.${what}LoadError` },
  subs: [{ text: error }],
})

function repoRows(repos: Source<string[]>): HealthRow[] {
  if (repos.state === 'loading') return []
  if (repos.state === 'error') return [loadError('repos', repos.error)]
  if (repos.value.length === 0) {
    return [{
      key: 'repos', tone: 'warn', title: { key: 'projectDetail.health.noRepos' }, subs: [],
      link: { target: 'repos', labelKey: 'projectDetail.health.linkRepos' },
    }]
  }
  return [{
    key: 'repos', tone: 'ok',
    title: { key: 'projectDetail.health.repos', params: { count: repos.value.length } },
    subs: [{ text: repos.value.join(SEP) }],
  }]
}

function accountRows(engine: 'claude' | 'codex', account: Source<string | null>): HealthRow[] {
  if (account.state === 'loading') return []
  if (account.state === 'error') return [loadError('account', account.error)]
  if (account.value === null) {
    return [{
      key: 'account', tone: 'warn',
      title: { key: `projectDetail.health.${engine}NoAccount` }, subs: [],
      link: { target: 'session', labelKey: 'projectDetail.health.linkSession' },
    }]
  }
  return [{
    key: 'account', tone: 'ok',
    title: { key: `projectDetail.health.${engine}Account`, params: { label: account.value } },
    subs: [{ key: 'projectDetail.health.ready' }],
  }]
}

function mcpRows(mcp: Source<McpHealth>): HealthRow[] {
  if (mcp.state === 'error') return [loadError('mcp', mcp.error)]
  if (mcp.state === 'loading') {
    return [{ key: 'mcp-busy', tone: 'busy', title: { key: 'projectDetail.health.mcpChecking' }, subs: [] }]
  }
  const { enabled, checking, failures } = mcp.value
  const rows: HealthRow[] = failures.map((f) => ({
    key: `mcp-${f.name}`,
    tone: 'warn' as const,
    title: { key: 'projectDetail.health.mcpFailed', params: { name: f.name } },
    subs: [{ key: 'projectDetail.health.mcpLastCheck', params: { time: clockText(f.at), message: f.message } }],
    link: { target: 'mcp' as const, labelKey: 'projectDetail.health.linkMcp' },
  }))
  if (checking) {
    rows.push({ key: 'mcp-busy', tone: 'busy', title: { key: 'projectDetail.health.mcpChecking' }, subs: [] })
  } else if (failures.length === 0 && enabled > 0) {
    rows.push({
      key: 'mcp', tone: 'ok', title: { key: 'projectDetail.health.mcpOk', params: { count: enabled } }, subs: [],
    })
  }
  return rows
}

function conflictRows(conflicts: HealthInput['conflicts']): HealthRow[] {
  if (conflicts.length === 0) return []
  return [{
    key: 'conflicts',
    tone: 'warn',
    title: { key: 'projectDetail.health.conflicts', params: { count: conflicts.length } },
    subs: conflicts.map((c) => ({
      key: 'projectDetail.health.conflictItem',
      params: { name: c.name, kind: c.kind === 'skill' ? 'Skill' : 'Rule' },
    })),
    link: { target: 'conflicts', labelKey: 'projectDetail.health.linkConflicts' },
    action: 'recheck',
  }]
}

function serverRows(servers: Source<string[]>): HealthRow[] {
  if (servers.state === 'loading') return []
  if (servers.state === 'error') return [loadError('servers', servers.error)]
  if (servers.value.length === 0) return []
  return [{
    key: 'servers', tone: 'ok',
    title: { key: 'projectDetail.health.servers', params: { count: servers.value.length } },
    subs: [{ text: servers.value.join(SEP) }],
  }]
}

/** The card's rows, top to bottom. A source still loading adds no row, so nothing flashes. */
export function healthRows(input: HealthInput): HealthRow[] {
  return [
    ...repoRows(input.repos),
    ...accountRows(input.engine, input.account),
    ...mcpRows(input.mcp),
    ...conflictRows(input.conflicts),
    ...serverRows(input.servers),
  ]
}
