// Dev servers on the Background Processes screen (`orchestration.dev_servers`): the
// shapes of src-tauri/src/procs/dev_servers.rs and the pure helpers of the card
// (tested in __tests__/devServers.test.mjs). Kept free of Vue / Tauri / `@/` imports.

export const DEV_SERVERS_FEATURE = 'orchestration.dev_servers'

/** `{ projectId }` → DevServerList. Errors when the file is broken. */
export const CMD_DEVSERVER_LIST = 'devserver_list'
/** `{ projectId, name, command }` → pid. `command` is the launch line the user confirmed. Audited. */
export const CMD_DEVSERVER_START = 'devserver_start'
/** `{ projectId, name }` → processes signalled. Audited. */
export const CMD_DEVSERVER_STOP = 'devserver_stop'
/** `{ projectId, name }` → DevServerLog. */
export const CMD_DEVSERVER_LOGS = 'devserver_logs'

export type DevServerStatus = 'running' | 'port_busy' | 'unknown' | 'stopped'

export interface DevServerView {
  name: string
  command: string
  args: string[]
  cwd: string
  port: number | null
  description: string | null
  /** The exact command Start runs (the same line the agent's `dev_server_start` returns). */
  launch: string
  log: string
  /** Processes writing the log; null when `lsof` cannot tell. */
  pids: number[] | null
  port_busy: boolean | null
  status: DevServerStatus
}

export interface DevServerList {
  config_path: string
  missing: boolean
  servers: DevServerView[]
  /** Entries left out of the file, one line each. */
  problems: string[]
}

export interface DevServerLog {
  text: string
  truncated: boolean
}

export const CONFIG_EXAMPLE = JSON.stringify(
  {
    version: 1,
    servers: [{ name: 'web', command: 'pnpm', args: ['dev'], cwd: 'apps/web', port: 3000, description: 'Vite dev server' }],
  },
  null,
  2,
)

/** The backend refuses a second copy anyway; this only hides a button that cannot work. */
export function canStart(s: DevServerView): boolean {
  return s.status === 'stopped' || s.status === 'unknown'
}

export function canStop(s: DevServerView): boolean {
  return s.status === 'running'
}

/** Keep the chosen project while it still exists, else the first one. */
export function pickProjectId(projectIds: string[], current: string): string {
  return projectIds.includes(current) ? current : (projectIds[0] ?? '')
}
