// Pure helpers behind the permission window's "Purpose" and "Impact" blocks (V3-31).
// Everything here is a guess from the tool name and its input — nothing is run —
// and carries no UI text: summaries are `{ key, params }` for the component to translate.

import { brokerAction, brokerArgv, brokerHost, brokerRisks, isBrokerRequest, type BrokerPromptInfo } from './brokerCommand.ts'

export type PermissionRisk = 'read' | 'write' | 'delete' | 'network'

export interface ImpactRequest {
  tool_name: string
  tool_input?: unknown
  cwd?: string | null
  once_only?: boolean
  broker?: BrokerPromptInfo | null
}

export interface ImpactFile {
  path: string
  added: number
  removed: number
}
export interface ImpactCommand {
  cmd: string
  risks: PermissionRisk[]
}
export interface ImpactNetwork {
  /** '' when it cannot be guessed (a git remote, a package registry, a search). */
  host: string
  method?: string
  kind?: 'git' | 'package'
}
export interface ImpactSummary {
  /** Key under `permission.impact.` */
  key: string
  params: Record<string, string | number>
}
export interface DiffPart {
  before: string
  after: string
}
export interface PermissionImpact {
  summary: ImpactSummary
  files: ImpactFile[]
  commands: ImpactCommand[]
  cwd: string | null
  network: ImpactNetwork[]
  mcpServer?: string
  mcpTool?: string
  risks: PermissionRisk[]
}

const RISK_ORDER: PermissionRisk[] = ['read', 'write', 'delete', 'network']

const READ_TOOLS = new Set(['Read', 'Grep', 'Glob', 'LS', 'NotebookRead'])
const WRITE_TOOLS = new Set(['Edit', 'Write', 'MultiEdit', 'NotebookEdit', 'apply_patch'])
const NETWORK_TOOLS = new Set(['WebFetch', 'WebSearch'])
// Claude's `Bash` plus the names Codex-style engines use for a shell call.
const SHELL_TOOLS = new Set(['Bash', 'shell', 'local_shell', 'exec_command', 'unified_exec', 'container.exec'])

const READ_COMMANDS = new Set([
  'ls', 'cat', 'head', 'tail', 'grep', 'rg', 'egrep', 'fgrep', 'wc', 'pwd', 'echo', 'printf', 'stat', 'file',
  'which', 'whoami', 'tree', 'du', 'df', 'env', 'printenv', 'sort', 'uniq', 'diff', 'cd', 'true', 'false',
  'test', 'date', 'basename', 'dirname', 'realpath', 'less', 'more', 'cut', 'tr', 'jq', 'uname', 'ps',
])
const READ_GIT = new Set(['status', 'log', 'diff', 'show', 'rev-parse', 'ls-files', 'blame', 'describe', 'remote'])
const DELETE_COMMANDS = new Set(['rm', 'rmdir', 'unlink', 'shred', 'trash'])
const NETWORK_COMMANDS = new Set(['curl', 'wget', 'ssh', 'scp', 'sftp', 'nc', 'ping', 'gh', 'ftp'])
const NETWORK_GIT = new Set(['push', 'pull', 'fetch', 'clone'])
const PKG_MANAGERS = new Set(['npm', 'pnpm', 'yarn', 'pip', 'pip3', 'bun'])
const PKG_INSTALL = new Set(['i', 'install', 'add', 'ci', 'update', 'up', 'upgrade'])
const WRAPPERS = new Set(['sudo', 'time', 'env', 'command', 'nohup', 'exec', 'nice'])

function asObject(v: unknown): Record<string, unknown> {
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {}
}
function str(v: unknown): string {
  return typeof v === 'string' ? v : ''
}
function countLines(s: string): number {
  return s ? s.split('\n').length : 0
}
function sortRisks(set: Set<PermissionRisk>): PermissionRisk[] {
  return RISK_ORDER.filter((r) => set.has(r))
}

/** Splits a shell line on `&&`, `||`, `;` and newlines, honouring quotes, `\` continuations and heredocs. */
export function splitShellCommands(cmd: string): string[] {
  const out: string[] = []
  let cur = ''
  let quote: '' | "'" | '"' = ''
  let heredoc: string | null = null
  const flush = () => {
    const t = cur.trim()
    if (t) out.push(t)
    cur = ''
  }
  let i = 0
  while (i < cmd.length) {
    const c = cmd[i]
    if (heredoc !== null) {
      // Swallow the heredoc body into the current command up to its terminator line.
      const nl = cmd.indexOf('\n', i)
      const line = nl === -1 ? cmd.slice(i) : cmd.slice(i, nl)
      cur += line + (nl === -1 ? '' : '\n')
      i = nl === -1 ? cmd.length : nl + 1
      if (line.trim() === heredoc) {
        heredoc = null
        flush()
      }
      continue
    }
    if (quote) {
      cur += c
      if (c === '\\' && quote === '"' && i + 1 < cmd.length) cur += cmd[++i]
      else if (c === quote) quote = ''
      i++
      continue
    }
    if (c === '\\' && cmd[i + 1] === '\n') {
      cur += ' '
      i += 2
      continue
    }
    if (c === '\\' && i + 1 < cmd.length) {
      cur += c + cmd[i + 1]
      i += 2
      continue
    }
    if (c === "'" || c === '"') {
      quote = c
      cur += c
      i++
      continue
    }
    if (c === '<' && cmd[i + 1] === '<' && cmd[i + 2] !== '<') {
      const m = /^<<-?\s*(['"]?)([A-Za-z_][\w-]*)\1/.exec(cmd.slice(i))
      if (m) {
        cur += m[0]
        i += m[0].length
        const nl = cmd.indexOf('\n', i)
        // The rest of this line still belongs to the command; the body starts on the next line.
        const rest = nl === -1 ? cmd.slice(i) : cmd.slice(i, nl)
        cur += rest + (nl === -1 ? '' : '\n')
        i = nl === -1 ? cmd.length : nl + 1
        heredoc = m[2]
        continue
      }
    }
    if (c === '\n' || c === ';') {
      flush()
      i++
      continue
    }
    if ((c === '&' && cmd[i + 1] === '&') || (c === '|' && cmd[i + 1] === '|')) {
      flush()
      i += 2
      continue
    }
    cur += c
    i++
  }
  flush()
  return out
}

/** Quote-aware split of one command into words (quotes removed). */
function tokenize(s: string): string[] {
  const words: string[] = []
  let cur = ''
  let has = false
  let quote: '' | "'" | '"' = ''
  for (let i = 0; i < s.length; i++) {
    const c = s[i]
    if (quote) {
      if (c === quote) quote = ''
      else if (c === '\\' && quote === '"' && i + 1 < s.length) cur += s[++i]
      else cur += c
      continue
    }
    if (c === "'" || c === '"') {
      quote = c
      has = true
    } else if (c === '\\' && i + 1 < s.length) {
      cur += s[++i]
      has = true
    } else if (/\s/.test(c)) {
      if (has || cur) words.push(cur)
      cur = ''
      has = false
    } else {
      cur += c
      has = true
    }
  }
  if (has || cur) words.push(cur)
  return words
}

/** Splits a command on pipes outside quotes. */
function splitPipes(s: string): string[] {
  const parts: string[] = []
  let cur = ''
  let quote = ''
  for (let i = 0; i < s.length; i++) {
    const c = s[i]
    if (quote) {
      if (c === quote) quote = ''
      cur += c
    } else if (c === "'" || c === '"') {
      quote = c
      cur += c
    } else if (c === '|') {
      parts.push(cur)
      cur = ''
    } else cur += c
  }
  parts.push(cur)
  return parts.map((p) => p.trim()).filter(Boolean)
}

function hostOf(word: string): string {
  const m = /^(?:[a-z][a-z0-9+.-]*:\/\/)?(?:[^@/\s]+@)?([a-z0-9.-]+\.[a-z]{2,}|localhost|\d{1,3}(?:\.\d{1,3}){3})(?::\d+)?(?:[/?#]|$)/i.exec(word)
  return m ? m[1].toLowerCase() : ''
}

interface SegmentInfo {
  risks: Set<PermissionRisk>
  network: ImpactNetwork[]
}

function classifySimple(part: string): SegmentInfo {
  const info: SegmentInfo = { risks: new Set(), network: [] }
  // Redirects to /dev/null or fd merges are not writes.
  const stripped = part.replace(/\d*>&\d+/g, '').replace(/\d*>>?\s*\/dev\/null/g, '')
  const hasRedirect = />>?\s*[^\s&>]/.test(stripped)
  let words = tokenize(stripped)
  while (words.length && (/^[A-Za-z_][A-Za-z0-9_]*=/.test(words[0]) || WRAPPERS.has(words[0]))) words = words.slice(1)
  const name = (words[0] ?? '').replace(/^.*\//, '')
  const args = words.slice(1)
  const flags = args.filter((a) => a.startsWith('-'))
  const sub = args.find((a) => !a.startsWith('-')) ?? ''

  if (/\b(drop|truncate)\s+(table|database|schema)\b/i.test(part) || /\bdelete\s+from\b/i.test(part)) {
    info.risks.add('delete')
  } else if (DELETE_COMMANDS.has(name)) {
    info.risks.add('delete')
  } else if (name === 'git') {
    if (sub === 'clean') info.risks.add('delete')
    else if (sub === 'reset' && args.includes('--hard')) info.risks.add('delete')
    else if (sub === 'branch' && (args.includes('-D') || args.includes('--delete'))) info.risks.add('delete')
    else if (NETWORK_GIT.has(sub)) {
      info.risks.add('network')
      info.network.push({ host: '', kind: 'git', method: sub })
      if (sub === 'push' && flags.some((f) => f === '-f' || f.startsWith('--force'))) info.risks.add('delete')
    } else if (READ_GIT.has(sub) || (sub === 'branch' && !hasRedirect)) info.risks.add('read')
    else info.risks.add('write')
  } else if (NETWORK_COMMANDS.has(name)) {
    info.risks.add('network')
    const host = args.map(hostOf).find(Boolean) ?? ''
    info.network.push({ host, method: name === 'curl' || name === 'wget' ? 'GET' : undefined })
  } else if (PKG_MANAGERS.has(name) && PKG_INSTALL.has(sub)) {
    info.risks.add('network')
    info.network.push({ host: '', kind: 'package' })
  } else if (name === 'find') {
    info.risks.add(flags.includes('-delete') ? 'delete' : flags.includes('-exec') ? 'write' : 'read')
  } else if (name === 'sed') {
    info.risks.add(flags.some((f) => f.startsWith('-i')) ? 'write' : 'read')
  } else if (READ_COMMANDS.has(name)) {
    info.risks.add('read')
  } else {
    // Unknown command: assume it writes (safer than assuming it only reads).
    info.risks.add('write')
  }
  if (hasRedirect && !info.risks.has('delete')) {
    info.risks.delete('read')
    info.risks.add('write')
  }
  return info
}

function classifyCommand(cmd: string): SegmentInfo {
  const total: SegmentInfo = { risks: new Set(), network: [] }
  for (const part of splitPipes(cmd)) {
    const s = classifySimple(part)
    s.risks.forEach((r) => total.risks.add(r))
    total.network.push(...s.network)
  }
  if (!total.risks.size) total.risks.add('write')
  return total
}

function mcpParts(name: string): { server: string; tool: string } | null {
  if (!name.startsWith('mcp__')) return null
  const [server = '', ...rest] = name.slice(5).split('__')
  return { server, tool: rest.join('__') }
}

function shellCommand(input: Record<string, unknown>): string {
  const c = input['command'] ?? input['cmd']
  if (Array.isArray(c)) return c.map(String).join(' ')
  return str(c)
}

/** Risk labels for a request. A request can carry several. */
export function classifyPermissionRisk(req: ImpactRequest): PermissionRisk[] {
  if (isBrokerRequest(req)) return brokerRisks(req)
  const name = req.tool_name
  const set = new Set<PermissionRisk>()
  if (READ_TOOLS.has(name)) set.add('read')
  else if (WRITE_TOOLS.has(name)) set.add('write')
  else if (NETWORK_TOOLS.has(name)) set.add('network')
  else if (SHELL_TOOLS.has(name)) {
    const cmds = splitShellCommands(shellCommand(asObject(req.tool_input)))
    if (!cmds.length) set.add('write')
    for (const c of cmds) classifyCommand(c).risks.forEach((r) => set.add(r))
  } else if (name.startsWith('mcp__')) {
    set.add('network')
    const tool = mcpParts(name)?.tool ?? ''
    set.add(/^(get|list|read|search|fetch|find|describe|query)([_-]|$)/i.test(tool) ? 'read' : 'write')
  } else set.add('write')
  return sortRisks(set)
}

/** Lines a patch string (`*** Update File: …` blocks, Codex apply_patch) adds and removes, per file. */
function parsePatch(patch: string): ImpactFile[] {
  const files: ImpactFile[] = []
  let cur: ImpactFile | null = null
  for (const line of patch.split('\n')) {
    const m = /^\*\*\* (?:Add|Update|Delete) File:\s*(.+)$/.exec(line)
    if (m) {
      cur = { path: m[1].trim(), added: 0, removed: 0 }
      files.push(cur)
    } else if (cur && line.startsWith('+') && !line.startsWith('+++')) cur.added++
    else if (cur && line.startsWith('-') && !line.startsWith('---')) cur.removed++
  }
  return files
}

/** Edit / MultiEdit / Write as before→after pairs, for the inline diff. */
export function permissionDiffParts(req: ImpactRequest): DiffPart[] {
  const inp = asObject(req.tool_input)
  if (req.tool_name === 'Edit') {
    const before = str(inp['old_string'])
    const after = str(inp['new_string'])
    if (before || after) return [{ before, after }]
  }
  if (req.tool_name === 'MultiEdit' && Array.isArray(inp['edits'])) {
    return (inp['edits'] as unknown[])
      .map((e) => asObject(e))
      .map((e) => ({ before: str(e['old_string']), after: str(e['new_string']) }))
      .filter((d) => d.before || d.after)
  }
  if (req.tool_name === 'Write') {
    const after = str(inp['content'])
    if (after) return [{ before: '', after }]
  }
  return []
}

function filesOf(req: ImpactRequest): ImpactFile[] {
  const inp = asObject(req.tool_input)
  const path = str(inp['file_path']) || str(inp['path']) || str(inp['notebook_path'])
  const parts = permissionDiffParts(req)
  if (parts.length) {
    return [{
      path,
      added: parts.reduce((n, p) => n + countLines(p.after), 0),
      removed: parts.reduce((n, p) => n + countLines(p.before), 0),
    }]
  }
  if (req.tool_name === 'NotebookEdit') return [{ path, added: countLines(str(inp['new_source'])), removed: 0 }]
  const patch = str(req.tool_input) || str(inp['input']) || str(inp['patch'])
  if (patch) {
    const fromPatch = parsePatch(patch)
    if (fromPatch.length) return fromPatch
  }
  const changes = inp['changes']
  if (changes && typeof changes === 'object' && !Array.isArray(changes)) {
    return Object.keys(changes).map((p) => ({ path: p, added: 0, removed: 0 }))
  }
  return path ? [{ path, added: 0, removed: 0 }] : []
}

/** What the request will touch, in a shape the Impact block can print. */
export function permissionImpact(req: ImpactRequest): PermissionImpact {
  const name = req.tool_name
  const inp = asObject(req.tool_input)
  const risks = classifyPermissionRisk(req)
  const base = { files: [] as ImpactFile[], commands: [] as ImpactCommand[], network: [] as ImpactNetwork[], cwd: req.cwd ?? null, risks }

  if (WRITE_TOOLS.has(name)) {
    const files = filesOf(req)
    const added = files.reduce((n, f) => n + f.added, 0)
    const removed = files.reduce((n, f) => n + f.removed, 0)
    const many = files.length > 1
    const key = name === 'Write' ? 'sumWrite' : many ? 'sumEditMany' : 'sumEditOne'
    return { ...base, files, summary: { key, params: { n: files.length, added, removed } } }
  }
  if (SHELL_TOOLS.has(name)) {
    const cmds = splitShellCommands(shellCommand(inp))
    const commands = cmds.map((cmd) => ({ cmd, risks: sortRisks(classifyCommand(cmd).risks) }))
    const network = cmds.flatMap((c) => classifyCommand(c).network)
    return { ...base, commands, network, summary: { key: commands.length > 1 ? 'sumRunMany' : 'sumRunOne', params: { n: commands.length } } }
  }
  if (NETWORK_TOOLS.has(name)) {
    const host = hostOf(str(inp['url']))
    const network = [{ host, method: 'GET' }]
    return { ...base, network, summary: host ? { key: 'sumNet', params: { host } } : { key: 'sumNetNoHost', params: {} } }
  }
  const mcp = mcpParts(name)
  if (mcp) {
    return { ...base, mcpServer: mcp.server, mcpTool: mcp.tool, summary: { key: 'sumMcp', params: { tool: mcp.tool || name, server: mcp.server } } }
  }
  if (READ_TOOLS.has(name)) {
    const target = str(inp['file_path']) || str(inp['path']) || str(inp['pattern'])
    return { ...base, summary: { key: 'sumRead', params: { target } } }
  }
  if (isBrokerRequest(req)) {
    // A broker CLI always talks to a server, so the Network row never reads "None".
    const argv = brokerArgv(req)
    const { key, params } = brokerAction(req)
    const network = key === 'gitPush'
      ? [{ host: '', kind: 'git' as const, method: 'push' }]
      : [{ host: brokerHost(req) ?? argv.map(hostOf).find(Boolean) ?? '' }]
    return { ...base, network, summary: { key: `action.${key}`, params } }
  }
  return { ...base, summary: { key: 'sumOther', params: { tool: name } } }
}

const PURPOSE_MAX = 240

function clampPurpose(s: string): string | null {
  const t = s.trim()
  if (!t) return null
  return t.length > PURPOSE_MAX ? t.slice(0, PURPOSE_MAX - 1).trimEnd() + '…' : t
}

interface EntryLike {
  kind: string
  text?: string
  name?: string
  input?: unknown
}

function sameInput(a: unknown, b: unknown): boolean {
  try {
    return JSON.stringify(a) === JSON.stringify(b)
  } catch {
    return false
  }
}

/** The last paragraph of the nearest agent text before index `from`, skipping tool calls and thinking. */
function textBefore(entries: EntryLike[], from: number): string | null {
  for (let i = from - 1; i >= 0; i--) {
    const e = entries[i]
    if (e.kind === 'tool' || e.kind === 'thinking') continue
    if (e.kind !== 'text') return null
    const paras = (e.text ?? '').split(/\n{2,}/).map((p) => p.trim()).filter(Boolean)
    return clampPurpose(paras[paras.length - 1] ?? '')
  }
  return null
}

/**
 * Why the agent wants this: the tool's own description, else the agent's last words
 * before the call. No id ties a request to a log entry, so the match is name + input;
 * when the call isn't logged yet, trailing agent text is used. `null` hides the line.
 */
export function permissionPurpose(req: ImpactRequest, entries: readonly EntryLike[] | null | undefined): string | null {
  const own = clampPurpose(str(asObject(req.tool_input)['description']))
  if (own) return own
  const list = entries ?? []
  for (let i = list.length - 1; i >= 0; i--) {
    const e = list[i]
    if (e.kind === 'tool' && e.name === req.tool_name && sameInput(e.input, req.tool_input)) return textBefore(list as EntryLike[], i)
  }
  return list.length ? textBefore(list as EntryLike[], list.length) : null
}

// ── Header of the permission window (RT-21, #4) ─────────────────────────────

const HEADLINE_MAX = 120

export interface PermissionHeadline {
  text: string
  /** `purpose`: why the agent wants it; `target`: the tool and what it acts on. */
  source: 'purpose' | 'target'
  /** The whole purpose fits the headline, so the body need not repeat it. */
  full: boolean
}

function clampLine(s: string, max: number): string {
  return s.length > max ? s.slice(0, max - 1).trimEnd() + '…' : s
}

/** What the request acts on, short: a file, a host, an MCP server, the first command. */
function targetOf(req: ImpactRequest): string {
  const inp = asObject(req.tool_input)
  const mcp = mcpParts(req.tool_name)
  if (mcp) return mcp.server
  if (NETWORK_TOOLS.has(req.tool_name)) return hostOf(str(inp['url'])) || str(inp['query'])
  if (SHELL_TOOLS.has(req.tool_name)) return splitShellCommands(shellCommand(inp))[0] ?? ''
  return str(inp['file_path']) || str(inp['path']) || str(inp['notebook_path']) || str(inp['pattern'])
    || filesOf(req)[0]?.path || ''
}

/**
 * The permission header's title: the purpose (first line, cut to one line), else
 * the tool's name and its target. Null when neither says anything, so the
 * engine's own title stays.
 */
export function permissionHeadline(
  req: ImpactRequest & { display_name?: string | null },
  purpose: string | null | undefined,
): PermissionHeadline | null {
  const p = (purpose ?? '').trim()
  const first = p.split('\n')[0].trim()
  if (first) {
    const text = clampLine(first, HEADLINE_MAX)
    return { text, source: 'purpose', full: text === p }
  }
  const target = targetOf(req).trim().split('\n')[0]
  if (!target) return null
  const name = mcpParts(req.tool_name)?.tool || req.display_name || req.tool_name
  return { text: clampLine(`${name} · ${target}`, HEADLINE_MAX), source: 'target', full: false }
}

/** One short note per risky kind (write, delete, network), as keys under `permission.impact.`. */
export function permissionRiskNotes(impact: PermissionImpact): ImpactSummary[] {
  const out: ImpactSummary[] = []
  if (impact.risks.includes('write')) out.push({ key: 'riskNoteWrite', params: {} })
  if (impact.risks.includes('delete')) out.push({ key: 'riskNoteDelete', params: {} })
  if (impact.risks.includes('network')) {
    const hosts = [...new Set(impact.network.map((n) => n.host).filter(Boolean))]
    out.push(hosts.length ? { key: 'riskNoteNetworkHost', params: { host: hosts.join(', ') } } : { key: 'riskNoteNetwork', params: {} })
  }
  return out
}

/** The Impact block starts open when the request writes, deletes or goes over the network. */
export function impactOpensItself(risks: readonly PermissionRisk[]): boolean {
  return risks.some((r) => r !== 'read')
}
