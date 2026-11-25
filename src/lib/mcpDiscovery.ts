// MCP servers configured on this machine outside Puppeteer (backend
// `mcp_discovery`): Claude / Codex config files, claude.ai connectors of each
// Claude profile, plugin and managed servers, other AI apps. The two MCP views
// list them — Settings › Connections › MCP the global ones, a project's MCP tab
// its own plus the global ones — with what the user can do on each row (import,
// open the managed copy, sync it). Shapes mirror `mcp_discovery/state.rs`; no
// row ever carries an env / header VALUE, a connector's id or url, or a login.
// Free of Vue / `@/` imports so the helpers can be unit-tested with `node --test`.

export const DISCOVERY_COMMANDS = {
  list: 'list_mcp_discovery',
  reload: 'reload_mcp_discovery',
  importRows: 'import_discovered_mcp',
  sync: 'sync_discovered_mcp',
  setConnectorEnabled: 'set_mcp_connector_enabled',
} as const

/** Emitted after a scan, an import / sync, and when the engine probes start or end: `{ probing }`. */
export const DISCOVERY_CHANGED_EVENT = 'mcp_discovery:changed'

/** `transport` of a managed claude.ai connector in the MCP list: no command / url to edit, test or export. */
export const CONNECTOR_TRANSPORT = 'claudeai'

/** Selection id of the "Global connections" entry pinned on top of the MCP sidebar (never a server UUID). */
export const GLOBAL_CONNECTIONS_ID = 'discovery:global'

/** The Claude profile ref of the CLI's own config dir (no app account). */
export const GLOBAL_PROFILE = 'global'

export type SourceId = 'S1' | 'S2' | 'S3' | 'S4' | 'S5' | 'S6' | 'S7' | 'S8' | 'S9'
export type RowState = 'not_managed' | 'managed' | 'changed' | 'blocked' | 'unreadable'
export type ConflictCode = 'C1' | 'C2' | 'C3' | 'C4'
export type DiscoveryTransport = 'stdio' | 'http' | 'sse' | 'connector' | 'unknown'

export interface ProfileRef {
  /** App account id, or `global`. */
  ref: string
  engine: 'claude' | 'codex'
  /** The app account's label; null for `global`. */
  label: string | null
}

/** Key names only, like the import's summary. */
export interface DiscoverySummary {
  transport: string
  command: string | null
  args: string[]
  url: string | null
  env_keys: string[]
  header_keys: string[]
}

export interface DiscoveryRow {
  /** `<S-id>|<source ref>|<name>`. */
  id: string
  source: SourceId
  scope: 'global' | 'project'
  /** The file, or `claude.ai` for a connector. */
  originLabel: string
  profile: ProfileRef | null
  projectId: string | null
  /** Other apps (S9): the import adapter id (`cursor`, `vscode`…). */
  app: string | null
  name: string
  /** The name an import gives the copy (a connector: its run key, `claude_ai_Slack`). */
  importName: string
  transport: DiscoveryTransport
  summary: DiscoverySummary | null
  /** Probe rows: `connected`, `needs-auth`, `failed`… */
  engineStatus: string | null
  importable: boolean
  state: RowState
  /** The managed copy in the MCP list. */
  serverId: string | null
  problem: string | null
  conflicts: ConflictCode[]
  /** Managed and on app-wide (project / session switches still apply). */
  loadedInRuns: boolean
}

/** A source that could not be read at all. */
export interface DiscoveryProblem {
  source: SourceId
  /** The file, or `claude.ai` for a failed connector probe. */
  label: string
  message: string
  profile: ProfileRef | null
  projectId: string | null
  app: string | null
}

export interface ClaudeProfile extends ProfileRef {
  engine: 'claude'
  /** false: the login could not be read (C4). */
  known: boolean
  sameLoginAs: string[]
  conflicts: ConflictCode[]
}

export interface DiscoveryView {
  global: DiscoveryRow[]
  /** Empty without a project. */
  project: DiscoveryRow[]
  blockedCount: number
  problems: DiscoveryProblem[]
  profiles: ClaudeProfile[]
  scannedAt: string | null
  probedAt: string | null
  /** Engine probes (connectors, plugin and managed servers) still running. */
  probing: boolean
}

/** One answer of `import_discovered_mcp` (the import's outcome shape). */
export interface DiscoveryImportOutcome {
  id: string
  kind: string | null
  status: 'imported' | 'skipped' | 'failed'
  name: string | null
  message: string | null
}

/** What a row offers, by state. */
export type RowAction = 'import' | 'open' | 'sync' | 'blocked' | 'problem'

export function rowAction(row: Pick<DiscoveryRow, 'state' | 'importable' | 'serverId'>): RowAction {
  switch (row.state) {
    case 'not_managed':
      return row.importable ? 'import' : 'blocked'
    case 'managed':
      return row.serverId ? 'open' : 'blocked'
    case 'changed':
      return row.serverId ? 'sync' : 'blocked'
    case 'unreadable':
      return 'problem'
    default:
      return 'blocked'
  }
}

/** A row a run would load if Puppeteer managed it (same rule as the backend's `blockedCount`;
 *  other apps' rows, S9, are loaded by no engine). */
export function isBlockedRow(row: Pick<DiscoveryRow, 'source' | 'state'>): boolean {
  return row.source !== 'S9' && (row.state === 'not_managed' || row.state === 'blocked' || row.state === 'unreadable')
}

export function blockedCountOf(rows: readonly Pick<DiscoveryRow, 'source' | 'state'>[]): number {
  return rows.filter(isBlockedRow).length
}

/** The claude.ai connectors of one Claude profile, with its login labels and probe failure. */
export interface ConnectorGroup {
  profile: ProfileRef
  /** false: the login could not be read (C4); true when the profile is not in `profiles`. */
  known: boolean
  /** C1 / C2 / C4 of the profile (C3 sits on its row). */
  conflicts: ConflictCode[]
  rows: DiscoveryRow[]
  /** Connector detection failed for it (AC-015): none of its connectors can be used. */
  problems: DiscoveryProblem[]
}

export interface SplitRows {
  /** Every row but the connectors, in the backend's order. */
  servers: DiscoveryRow[]
  connectors: ConnectorGroup[]
  /** Unreadable sources other than a connector probe. */
  problems: DiscoveryProblem[]
}

const PROFILE_CONFLICTS: readonly ConflictCode[] = ['C1', 'C2', 'C4']

/**
 * Splits a section into its servers and one connector group per Claude profile.
 * Every profile of `profiles` gets a group, even without connectors, so a login
 * that cannot be read or a failed probe still shows; groups follow `profiles`,
 * then any profile only the rows name.
 */
export function splitRows(
  rows: readonly DiscoveryRow[],
  profiles: readonly ClaudeProfile[],
  problems: readonly DiscoveryProblem[],
): SplitRows {
  const groups = new Map<string, ConnectorGroup>()
  for (const p of profiles) {
    groups.set(p.ref, {
      profile: { ref: p.ref, engine: 'claude', label: p.label },
      known: p.known,
      conflicts: p.conflicts.filter((c) => PROFILE_CONFLICTS.includes(c)),
      rows: [],
      problems: [],
    })
  }
  const servers: DiscoveryRow[] = []
  for (const row of rows) {
    if (row.source !== 'S1' || !row.profile) {
      servers.push(row)
      continue
    }
    let group = groups.get(row.profile.ref)
    if (!group) {
      group = { profile: row.profile, known: true, conflicts: [], rows: [], problems: [] }
      groups.set(row.profile.ref, group)
    }
    group.rows.push(row)
    for (const c of row.conflicts) {
      if (PROFILE_CONFLICTS.includes(c) && !group.conflicts.includes(c)) group.conflicts.push(c)
    }
  }
  const rest: DiscoveryProblem[] = []
  for (const p of problems) {
    const group = p.source === 'S1' && p.profile ? groups.get(p.profile.ref) : undefined
    if (group) group.problems.push(p)
    else rest.push(p)
  }
  for (const g of groups.values()) g.conflicts.sort()
  return { servers, connectors: [...groups.values()], problems: rest }
}

/** Conflict labels a connector row shows itself; the profile ones sit on its group. */
export function rowConflicts(row: Pick<DiscoveryRow, 'source' | 'conflicts'>): ConflictCode[] {
  return row.source === 'S1' ? row.conflicts.filter((c) => !PROFILE_CONFLICTS.includes(c)) : row.conflicts
}

/** C3 drops the connector from runs and C4 hides a shared login: worth a warning; C1 / C2 are only facts. */
export function conflictTone(code: ConflictCode): 'warning' | 'info' {
  return code === 'C3' || code === 'C4' ? 'warning' : 'info'
}

/** The two groups of a project's view: its own sources, then the global ones, each with its problems. */
export function projectSections(view: Pick<DiscoveryView, 'project' | 'global' | 'problems'>): {
  key: 'project' | 'global'
  rows: DiscoveryRow[]
  problems: DiscoveryProblem[]
}[] {
  return [
    { key: 'project', rows: view.project, problems: view.problems.filter((p) => p.projectId !== null) },
    { key: 'global', rows: view.global, problems: view.problems.filter((p) => p.projectId === null) },
  ]
}

/** Whether a server of the MCP list is a claude.ai connector (switched by `set_mcp_connector_enabled`). */
export function isConnectorServer(server: { transport: string }): boolean {
  return server.transport === CONNECTOR_TRANSPORT
}

/** Cache key of a view: '' for Settings, else the project id. */
export function viewKey(projectId: string | null | undefined): string {
  return projectId ?? ''
}

/** Counts per status of an import, for the summary toast. */
export function outcomeCounts(outcomes: readonly Pick<DiscoveryImportOutcome, 'status'>[]): {
  imported: number
  skipped: number
  failed: number
} {
  const count = (s: DiscoveryImportOutcome['status']) => outcomes.filter((o) => o.status === s).length
  return { imported: count('imported'), skipped: count('skipped'), failed: count('failed') }
}
