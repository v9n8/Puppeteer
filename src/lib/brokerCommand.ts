// Pure helpers behind a credential-broker permission prompt (gh, glab, git, aws,
// az, terraform): a short action name for the title, the command's risks and its
// host. Nothing is run; titles go through a passed-in `t`, so no UI text lives here.

import type { PermissionRisk } from './permissionImpact.ts'

export type Translate = (key: string, params?: Record<string, unknown>) => string

export interface BrokerReasonPart {
  code: string
  params?: Record<string, string>
}

/** The `broker` part of a permission event: the CLI's argv after the tool name, the reason as codes. */
export interface BrokerPromptInfo {
  argv?: string[]
  reason?: BrokerReasonPart[]
  allow_for_run?: boolean
}

export interface BrokerRequest {
  tool_name: string
  tool_input?: unknown
  once_only?: boolean
  broker?: BrokerPromptInfo | null
}

export interface BrokerAction {
  /** Key under `permission.impact.action.` */
  key: string
  params: Record<string, string>
}

const PATH_MAX = 60
const WORDS_MAX = 40
const TITLE_MAX = 80

// Flags of `gh api` / `glab api` that take a value, as the backend's read check knows them.
const API_VALUE_FLAGS = new Set([
  '-X', '--method', '-f', '--raw-field', '-F', '--field', '-H', '--header', '-q', '--jq',
  '-t', '--template', '--hostname', '--cache', '-p', '--preview', '--input',
])
const AWS_GLOBAL_VALUE_FLAGS = new Set([
  '--endpoint-url', '--output', '--query', '--profile', '--region', '--color', '--ca-bundle',
  '--cli-read-timeout', '--cli-connect-timeout', '--cli-binary-format', '--cli-error-format',
])
const AZ_GLOBAL_VALUE_FLAGS = new Set(['--output', '-o', '--query', '--subscription'])
const AZ_GLOBAL_BOOL_FLAGS = new Set(['--debug', '--verbose', '--only-show-errors', '--help', '-h'])
const PR_ACTIONS = new Set(['merge', 'close', 'edit', 'review'])
const ISSUE_ACTIONS = new Set(['edit', 'close'])
const READ_GROUPS = new Set(['pr', 'mr', 'issue', 'repo', 'project', 'run', 'ci'])
const READ_ACTIONS = new Set(['list', 'view', 'diff', 'status', 'checks'])

function clamp(s: string, max: number): string {
  return s.length > max ? s.slice(0, max - 1).trimEnd() + '…' : s
}

function cap(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1)
}

/** Only the broker marks its prompts once-only; engine prompts never are. */
export function isBrokerRequest(r: BrokerRequest): boolean {
  return r.once_only === true
}

/** The argv after the tool name: the event's own, else the shown command split on spaces. */
export function brokerArgv(r: BrokerRequest): string[] {
  const own = r.broker?.argv
  if (Array.isArray(own)) return own
  const inp = r.tool_input && typeof r.tool_input === 'object' ? (r.tool_input as Record<string, unknown>) : {}
  const cmd = typeof inp['command'] === 'string' ? inp['command'] : ''
  const words = cmd.split(/\s+/).filter(Boolean)
  return words[0] === r.tool_name ? words.slice(1) : words
}

function words(argv: string[]): string[] {
  return argv.filter((a) => !a.startsWith('-'))
}

interface ApiCall {
  endpoint: string
  method: string | null
  fields: string[]
}

/** Endpoint, explicit method and body fields of an `api` call (argv after `api`). */
function apiCall(rest: string[]): ApiCall {
  const call: ApiCall = { endpoint: '', method: null, fields: [] }
  for (let i = 0; i < rest.length; i++) {
    const tok = rest[i]
    if (!tok.startsWith('-')) {
      if (!call.endpoint) call.endpoint = tok
      continue
    }
    let flag = tok
    let value: string | undefined
    const eq = tok.indexOf('=')
    if (tok.startsWith('--') && eq > 0) {
      flag = tok.slice(0, eq)
      value = tok.slice(eq + 1)
    } else if (!tok.startsWith('--') && tok.length > 2) {
      flag = tok.slice(0, 2)
      value = tok.slice(2)
    }
    // An unknown flag is taken as value-less, as the backend does.
    if (!API_VALUE_FLAGS.has(flag)) continue
    if (value === undefined) value = rest[++i] ?? ''
    if (flag === '-X' || flag === '--method') call.method = value.toUpperCase()
    else if (['-f', '--raw-field', '-F', '--field'].includes(flag)) call.fields.push(value)
  }
  return call
}

function isGraphql(endpoint: string): boolean {
  return endpoint.replace(/^\/+/, '').toLowerCase() === 'graphql'
}

function isMutation(call: ApiCall): boolean {
  return call.fields.some((f) => f.startsWith('query=') && /mutation/i.test(f))
}

/** GET unless `-X` says otherwise or a body field makes it POST. */
function apiMethod(call: ApiCall): string {
  return call.method ?? (call.fields.length ? 'POST' : 'GET')
}

/** `(service, operation)` after skipping aws global flags and their values. */
function awsPath(argv: string[]): string[] {
  const path: string[] = []
  for (let i = 0; i < argv.length && path.length < 2; i++) {
    const a = argv[i]
    if (!a.startsWith('-')) path.push(a.toLowerCase())
    else if (AWS_GLOBAL_VALUE_FLAGS.has(a)) i++
  }
  return path
}

/** The az command words, up to the first command-specific flag. */
function azPath(argv: string[]): string[] {
  const path: string[] = []
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i]
    if (!a.startsWith('-')) {
      path.push(a.toLowerCase())
      continue
    }
    const name = a.split('=')[0]
    if (AZ_GLOBAL_VALUE_FLAGS.has(name)) {
      if (!a.includes('=')) i++
    } else if (!AZ_GLOBAL_BOOL_FLAGS.has(name)) break
  }
  return path
}

function flagValue(argv: string[], names: string[]): string | null {
  for (let i = 0; i < argv.length; i++) {
    const [name, attached] = argv[i].split(/=(.*)/s)
    if (names.includes(name)) return attached ?? argv[i + 1] ?? null
  }
  return null
}

function generic(tool: string, argv: string[]): BrokerAction {
  return { key: 'generic', params: { tool, words: clamp(words(argv).slice(0, 2).join(' '), WORDS_MAX) } }
}

function ghAction(tool: 'gh' | 'glab', argv: string[]): BrokerAction {
  const provider = tool === 'gh' ? 'GitHub' : 'GitLab'
  const w = words(argv)
  const [group, action = '', next = ''] = w
  if (group === 'api') {
    const call = apiCall(argv.slice(argv.indexOf('api') + 1))
    if (isGraphql(call.endpoint)) {
      return { key: isMutation(call) ? 'graphqlWrite' : 'graphqlRead', params: { provider, tool } }
    }
    if (call.endpoint) {
      const method = apiMethod(call)
      const key = method === 'DELETE' ? 'apiDelete' : method === 'GET' || method === 'HEAD' ? 'apiRead' : 'apiWrite'
      return { key, params: { provider, path: clamp(call.endpoint, PATH_MAX) } }
    }
  }
  const n = /^\d+$/.test(next) ? next : ''
  if (group === (tool === 'gh' ? 'pr' : 'mr')) {
    const kind = tool === 'gh' ? 'PR' : 'MR'
    if (action === 'create') return { key: 'prCreate', params: { kind } }
    if (PR_ACTIONS.has(action)) return { key: `pr${cap(action)}${n ? 'N' : ''}`, params: n ? { kind, n } : { kind } }
  }
  if (group === 'issue') {
    if (action === 'create') return { key: 'issueCreate', params: {} }
    if (ISSUE_ACTIONS.has(action)) return { key: `issue${cap(action)}${n ? 'N' : ''}`, params: n ? { n } : {} }
  }
  return generic(tool, argv)
}

/** The action a broker command performs, as a key and its parameters. */
export function brokerAction(r: BrokerRequest): BrokerAction {
  const tool = r.tool_name
  const argv = brokerArgv(r)
  if (tool === 'gh' || tool === 'glab') return ghAction(tool, argv)
  const first = words(argv)[0] ?? ''
  if (tool === 'git' && first === 'push') return { key: 'gitPush', params: {} }
  if (tool === 'terraform' && first === 'apply') return { key: 'terraformApply', params: {} }
  if (tool === 'terraform' && first === 'destroy') return { key: 'terraformDestroy', params: {} }
  if (tool === 'az' && azPath(argv)[0] === 'rest') return { key: 'azRest', params: {} }
  if (tool === 'aws') {
    const [service, operation] = awsPath(argv)
    if (service && operation) return { key: 'aws', params: { service, operation } }
  }
  return generic(tool, argv)
}

const READ: PermissionRisk[] = ['read', 'network']
const WRITE: PermissionRisk[] = ['write', 'network']
const DELETE: PermissionRisk[] = ['delete', 'network']

function cloudRisks(tool: string, argv: string[]): PermissionRisk[] | null {
  if (tool === 'aws') {
    const [service = '', op = ''] = awsPath(argv)
    if (/^(get|list|describe)-/.test(op)) return READ
    if (/^(delete|remove|terminate)-/.test(op) || (service === 's3' && (op === 'rm' || op === 'rb'))) return DELETE
  }
  if (tool === 'az') {
    const path = azPath(argv)
    const last = path[path.length - 1] ?? ''
    if (path[0] === 'rest') return (flagValue(argv, ['--method', '-m']) ?? '').toUpperCase() === 'GET' ? READ : WRITE
    if (last === 'list' || last === 'show') return READ
    if (last === 'delete') return DELETE
  }
  return null
}

/** Risk labels of a broker command; every one goes over the network. */
export function brokerRisks(r: BrokerRequest): PermissionRisk[] {
  const tool = r.tool_name
  const argv = brokerArgv(r)
  const { key } = brokerAction(r)
  if (key === 'graphqlRead' || key === 'apiRead') return READ
  if (key === 'apiDelete' || key === 'terraformDestroy') return DELETE
  if (key === 'gitPush') {
    return argv.some((a) => a === '-f' || a.startsWith('--force')) ? ['write', 'delete', 'network'] : WRITE
  }
  if (tool === 'gh' || tool === 'glab') {
    const [group = '', action = ''] = words(argv)
    if (READ_GROUPS.has(group) && READ_ACTIONS.has(action)) return READ
    if (action === 'delete') return DELETE
  }
  return cloudRisks(tool, argv) ?? WRITE
}

/** The API host a gh / glab command talks to; null when the command does not say. */
export function brokerHost(r: BrokerRequest): string | null {
  if (r.tool_name !== 'gh' && r.tool_name !== 'glab') return null
  const host = flagValue(brokerArgv(r), ['--hostname'])
  if (host) return host
  return r.tool_name === 'gh' ? 'api.github.com' : null
}

/** The prompt's title: the translated action name, cut to one short line. */
export function brokerTitle(r: BrokerRequest, t: Translate): string {
  const { key, params } = brokerAction(r)
  return clamp(t(`permission.impact.action.${key}`, params), TITLE_MAX)
}

/**
 * Why the broker asks, in the app's language: each reason part translated and
 * joined. Null when there are no parts or one has a code this build does not
 * know, so the caller shows the backend's English text instead of half a sentence.
 */
export function brokerReasonText(r: BrokerRequest, t: Translate, has: (key: string) => boolean): string | null {
  const parts = r.broker?.reason ?? []
  if (!parts.length) return null
  const keys = parts.map((p) => `permission.brokerReason.${p.code}`)
  if (!keys.every((k) => has(k))) return null
  return parts.map((p, i) => t(keys[i], p.params ?? {})).join(' · ')
}
