// A child switch under a parent level: app-wide > project > session. A parent
// that is off never rewrites the child's stored value; the switch just shows off
// and cannot be turned on, and switching the parent back on brings the stored
// value back. The backend enforces the same rule (commands/mcp_effective.rs).

/** The level that keeps a child off. */
export type LockLevel = 'app' | 'project'

export interface SwitchView {
  /** What the switch shows: the stored value while no parent locks it, else off. */
  checked: boolean
  disabled: boolean
  lockedBy: LockLevel | null
}

/** How a child switch shows, from its stored value and the level locking it (null: none). */
export function switchView(stored: boolean, lockedBy: LockLevel | null): SwitchView {
  return { checked: lockedBy === null && stored, disabled: lockedBy !== null, lockedBy }
}

/** The level that locks a child: app-wide wins over the project. */
export function parentLock(onAppWide: boolean, onForProject = true): LockLevel | null {
  if (!onAppWide) return 'app'
  if (!onForProject) return 'project'
  return null
}

/** The env key that makes an MCP server a Slack one: the app reads Slack threads with its token. */
export const SLACK_TOKEN_ENV = 'SLACK_MCP_XOXP_TOKEN'

/** Whether an MCP server holds a Slack token (its env keys name it). */
export function isSlackServer(server: { env_keys: readonly string[] }): boolean {
  return server.env_keys.includes(SLACK_TOKEN_ENV)
}
