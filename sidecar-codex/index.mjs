// Puppeteer Codex sidecar.
//
// Drives `codex app-server` (JSON-RPC over stdio) and bridges it to the Puppeteer
// Rust broker using the SAME protocol as the Claude sidecar, so `drain_sidecar`,
// the frontend stream renderer, and the permission modal all work UNCHANGED:
//
//   stdout (to broker):
//     - Claude-shaped stream-json messages (translated from codex notifications):
//         {type:"system",subtype:"init",session_id,model,cwd,tools}
//         {type:"assistant",message:{content:[{type:"text"|"thinking"|"tool_use",...}]}}
//         {type:"user",message:{content:[{type:"tool_result",tool_use_id,content}]}}
//         {type:"result",subtype:"success"}            ← ends the turn
//     - control, namespaced `_puppeteer_*`:
//         {type:"_puppeteer_ready"} / {type:"_puppeteer_done"} / {type:"_puppeteer_error",error}
//         {type:"_puppeteer_closed",code,signal}   ← session over (signal ⇒ crash)
//         {type:"_puppeteer_permission_request",requestId,tool_name,tool_input,title,description}
//         {type:"_puppeteer_turn_failed"}          ← a prompt never became a turn (no result follows)
//         {type:"_puppeteer_steered",text,images}  ← codex took a steer (turn/steer accepted)
//
//   stdin (from broker): one JSON command per line
//     {type:"prompt",text,options?}            → start a turn (after the one in flight, if any)
//     {type:"permission_response",requestId,decision}  ("allow" | "deny")
//     {type:"interrupt"} / {type:"abort"}
//
// Auth: `codex app-server` reads the same ChatGPT login as the codex CLI, so
// runs bill against the user's subscription — no API key.

import { spawn, spawnSync } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { mkdirSync, readFileSync, writeFileSync, writeSync } from 'node:fs'
import { homedir } from 'node:os'
import { dirname, join } from 'node:path'

// --- Orphan guard ------------------------------------------------------------
// Puppeteer spawns this sidecar as the leader of its own process group and says so
// with PUPPETEER_OWN_PROCESS_GROUP=1 (removed here so no child inherits it). If
// Puppeteer dies without stopping us (crash, force quit), we are re-parented and
// nobody reads our output any more: tear the whole group down (CLI, MCP
// servers, tool processes) rather than letting a turn run on, burning tokens,
// with nobody listening. Processes that left the group are listed by Puppeteer's
// background-process view at its next launch.
const OWN_GROUP = process.env.PUPPETEER_OWN_PROCESS_GROUP === '1'
delete process.env.PUPPETEER_OWN_PROCESS_GROUP
const PARENT_PID = process.ppid
let orphaned = false
function orphanTeardown() {
  if (orphaned) return
  orphaned = true
  // Survive our own group-wide SIGTERM long enough to follow up with SIGKILL.
  process.on('SIGTERM', () => {})
  try { process.kill(-process.pid, 'SIGTERM') } catch {}
  setTimeout(() => {
    try { process.kill(-process.pid, 'SIGKILL') } catch {}
    process.exit(1)
  }, 2000)
}
const parentGone = () => process.ppid !== PARENT_PID
if (OWN_GROUP && PARENT_PID > 1) {
  const watch = setInterval(() => {
    if (parentGone()) { clearInterval(watch); orphanTeardown() }
  }, Math.max(50, Number(process.env.PUPPETEER_PARENT_WATCH_MS) || 2000))
  watch.unref()
}
// A write to a closed stdout (EPIPE) is how the death of Puppeteer usually shows
// first; the re-parenting follows right after. Re-check then: orphaned → tear
// down, otherwise stop as before (the reader went away on purpose).
process.stdout.on('error', (err) => {
  if (err?.code !== 'EPIPE') throw err
  setTimeout(() => {
    if (OWN_GROUP && parentGone()) orphanTeardown()
    else process.exit(1)
  }, 300)
})


// A per-process nonce so permission `requestId`s are globally unique. Each Puppeteer
// prompt spawns a FRESH sidecar (a new `codex app-server` whose JSON-RPC id
// counter restarts at a low number), so a bare `codex-<rpcId>` would collide
// across prompts within one remote session — and the Host's session-scoped
// idempotency guard would reject the 2nd+ real approval as `duplicate_request`,
// silently dropping the user's Allow/Deny. Prefixing with this nonce prevents
// that collision (P0: remote Deny had no effect after the first turn).
const PROC_NONCE = randomUUID().slice(0, 8)

// Rust resolves the binary (`runs/codex_bin.rs`) and always sets the path, plus
// PUPPETEER_CODEX_VERSION when `--version` answered. Supported: codex-cli 0.133+.
const CODEX_BIN = process.env.PUPPETEER_CODEX_PATH || 'codex'
const RESUME_THREAD = process.env.PUPPETEER_RESUME_SESSION || null
// Turns to roll back from the end of the resumed thread (a Rewind), else 0.
const ROLLBACK_TURNS = RESUME_THREAD ? Math.max(0, Number.parseInt(process.env.PUPPETEER_ROLLBACK_TURNS, 10) || 0) : 0
// Fork (UX-28): the resumed thread is copied (thread/fork) and this run goes on
// in the copy, rolled back to the turns it keeps (counted from the start, so
// turns the source added since the fork are dropped too).
const FORK = !!RESUME_THREAD && process.env.PUPPETEER_FORK_SESSION === '1'
const FORK_KEEP_TURNS = FORK && /^\d+$/.test(process.env.PUPPETEER_FORK_KEEP_TURNS || '')
  ? Number(process.env.PUPPETEER_FORK_KEEP_TURNS)
  : null
const PERMISSION_MODE = process.env.PUPPETEER_PERMISSION_MODE || 'default'
const MODEL = process.env.PUPPETEER_CODEX_MODEL || null
// Reasoning effort (UX-16), sent on every turn/start (app-server v2: "for this
// turn and subsequent turns"). The app's levels are Claude's; Codex has no
// `max`, so that one is left out and the model's default applies.
const CODEX_EFFORTS = ['low', 'medium', 'high', 'xhigh']
const EFFORT = CODEX_EFFORTS.includes(process.env.PUPPETEER_CODEX_EFFORT) ? process.env.PUPPETEER_CODEX_EFFORT : null
// Tool-less session (translation engine): the prompt carries UNTRUSTED text
// (Slack messages, GitHub comments), so a prompt injection must not be able to
// run anything. Forces a read-only sandbox, turns off codex's exec/app/plugin
// features, disables every MCP server, and declines any approval request.
const NO_TOOLS = process.env.PUPPETEER_CODEX_NO_TOOLS === '1'
// Extra system framing (session orchestration prompt / a worker's role
// instruction) for a NEW thread: sent as `developerInstructions` on
// thread/start, after which it lives in the thread itself.
const DEVELOPER_INSTRUCTIONS = process.env.PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS || null
// One-time framing seed for a RESUMED thread that never got it at start. Codex
// ignores developerInstructions on thread/resume, so the broker hands us a
// ready-made <session-framing> block to prepend to this sidecar's first turn.
let SEED_FRAMING = process.env.PUPPETEER_CODEX_SEED_FRAMING || null
// Response style (UX-17), kept apart from the framing so that only the framing
// is reported as `_puppeteer_framing_seeded`. A NEW thread gets it after the
// framing in `developerInstructions`; a RESUMED thread whose style changed gets
// the broker's ready-made block prepended to its first turn, like SEED_FRAMING.
const STYLE_INSTRUCTIONS = process.env.PUPPETEER_CODEX_STYLE_INSTRUCTIONS || null
let SEED_STYLE = process.env.PUPPETEER_CODEX_SEED_STYLE || null
// Session runs: stay up after a turn until the broker closes stdin (it does once
// the turn's `result` leaves nothing queued), so a follow-up it hands over then
// still finds the session. Without it (translation), a turn's end ends the session.
const KEEP_OPEN = process.env.PUPPETEER_CODEX_KEEP_OPEN === '1'
// Per-call MCP tool timeout. The broker sets MCP_TOOL_TIMEOUT (ms) above the
// conductor's longest `session_wait`; Codex's own default (~60s) would cut a
// blocking wait short. Applied to every server, matching Claude's global env.
const MCP_TOOL_TIMEOUT_SEC = (() => {
  const ms = Number(process.env.MCP_TOOL_TIMEOUT)
  return Number.isFinite(ms) && ms > 0 ? Math.ceil(ms / 1000) : null
})()

function out(obj) { process.stdout.write(JSON.stringify(obj) + '\n') }
function ctrl(type, extra = {}) { out({ type, ...extra }) }

// --- Global safety net -------------------------------------------------------
// Surface EVERY failure to the user as a friendly `_puppeteer_error` (which the
// broker forwards to the run log as an error line) instead of letting Node
// print a raw stack trace to stderr and exit — so the UI shows a clear message
// rather than the process appearing to "crash".
function reportFatal(err, context) {
  const msg = String(err?.stack || err?.message || err)
  try { ctrl('_puppeteer_error', { error: context ? `${context}: ${msg}` : msg }) } catch {}
}
process.on('uncaughtException', (err) => { reportFatal(err, 'Uncaught exception'); setTimeout(() => process.exit(1), 50) })
process.on('unhandledRejection', (err) => { reportFatal(err, 'Unhandled rejection'); setTimeout(() => process.exit(1), 50) })

// Liveness heartbeat ({type:"_puppeteer_heartbeat"} every 30s, declared in
// `_puppeteer_ready.capabilities`), independent of any
// codex work, so the broker can tell a long silent command (heartbeats keep
// coming) from a dead or frozen bridge (they stop). unref: never keeps us alive.
const HEARTBEAT_MS = Math.max(50, Number(process.env.PUPPETEER_SIDECAR_HEARTBEAT_MS) || 30_000)
setInterval(() => ctrl('_puppeteer_heartbeat'), HEARTBEAT_MS).unref()

// Reading the working directory can throw EPERM on macOS when the project folder
// is in a TCC-protected location (Desktop/Documents/Downloads/iCloud), on a
// network/removable volume the app can't access, or was deleted after opening.
// Fall back to the path passed by the broker so the sidecar reports the problem
// instead of crashing at startup with `uv_cwd`.
function safeCwd() {
  try {
    return process.cwd()
  } catch (e) {
    const fallback = process.env.PUPPETEER_PROJECT_PATH || process.env.HOME || '/'
    ctrl('_puppeteer_error', {
      error:
        `Không truy cập được thư mục làm việc (${e?.code || 'EPERM'}). ` +
        `Thư mục dự án có thể nằm ở nơi macOS chặn quyền (Desktop/Documents/Downloads/iCloud), ` +
        `trên ổ mạng/ổ ngoài chưa được cấp quyền, hoặc đã bị xóa/di chuyển. ` +
        `Hãy cấp Full Disk Access cho Puppeteer trong System Settings → Privacy & Security, ` +
        `hoặc chuyển dự án sang thư mục khác (ví dụ ~/Projects). Đang tạm dùng: ${fallback}`,
    })
    return fallback
  }
}
const CWD = safeCwd()

// Diagnostic log line (level: error|warn|info|debug|trace) — rendered as a muted
// log entry in the UI, NOT as a system error. Use for codex tracing + sidecar notes.
function note(text, level = 'info') { ctrl('_puppeteer_log', { level, text }) }

// Forward a codex RateLimitSnapshot to the broker as the same `_puppeteer_usage`
// control message the Claude sidecar emits, tagged `provider:"codex"`. The Rust
// drain persists it as the latest Codex plan-usage state (the equivalent of
// Claude's `/usage`). Best-effort — never throws into the RPC loop.
function emitCodexRateLimits(snapshot) {
  if (!snapshot || typeof snapshot !== 'object') return
  const rl = snapshot.rateLimits || snapshot.rateLimitsByLimitId?.codex || snapshot
  if (!rl || typeof rl !== 'object') return
  if (!rl.primary && !rl.secondary) return
  out({ type: '_puppeteer_usage', usage: { provider: 'codex', rate_limits: rl } })
}

// codex app-server writes `tracing` records to stderr: ANSI-colored, timestamp-led,
// e.g. "2026-…Z  ERROR codex_core::tools::router: error=…". Strip the ANSI/timestamp
// and pull out the level so the UI can render it calmly instead of as a red error.
const ANSI_RE = /\x1B\[[0-9;]*m/g
const TRACE_RE = /^\s*\d{4}-\d\d-\d\dT[\d:.]+Z\s+(ERROR|WARN|INFO|DEBUG|TRACE)\s+([\s\S]*)$/
let lastLogLevel = 'info'

function classifyStderrLine(raw) {
  const clean = raw.replace(ANSI_RE, '').replace(/\s+$/, '')
  if (!clean.trim()) return
  // Benign: the user just declined a command approval. codex echoes this as an
  // ERROR in its tool router, but the rejected commandExecution item already
  // renders as ✗ in the UI — drop the redundant internal trace entirely.
  if (/rejected by user/i.test(clean)) return
  const m = clean.match(TRACE_RE)
  if (m) { lastLogLevel = m[1].toLowerCase(); note(m[2].trim(), lastLogLevel) }
  // continuation / non-tracing line → keep the prior record's level (don't spike to error)
  else note(clean.replace(/^\s+/, ''), lastLogLevel)
}

// ── centrally-managed MCP servers → codex `-c mcp_servers.*` overrides ─────
// PUPPETEER_CODEX_MCP is the resolved Record<name, config>. Rust forwards stdio and
// streamable HTTP servers to Codex. We turn each into TOML config override flags
// placed BEFORE 'app-server'.
//
// The config carries no secrets (it ends up on codex's command line). An HTTP
// server with secret headers comes with `puppeteer_headers_helper` — the argv of
// sidecar-proxy/puppeteer-mcp-launch.mjs, which asks the credential broker for them.
// Codex has no helper hook of its own, so we run it here and hand the values to
// codex via env (`env_http_headers`), never via `-c` — so unlike on Claude,
// those header values do sit in the codex process env. A server whose
// credentials can't be fetched is left out (with a log line) rather than started
// without them.

/** Run a server's headers helper; null (server skipped) when it fails. */
function helperHeaders(name, argv) {
  if (!Array.isArray(argv) || !argv.length) return null
  const r = spawnSync(argv[0], argv.slice(1), {
    encoding: 'utf8', timeout: 15_000, stdio: ['ignore', 'pipe', 'pipe'],
  })
  if (r.status === 0) {
    try {
      const headers = JSON.parse(r.stdout)
      if (headers && typeof headers === 'object') return headers
    } catch { /* fall through */ }
  }
  // The helper's stderr names the reason (e.g. a broker denial) — never a secret.
  note(`MCP server "${name}" skipped: could not get its credentials (${(r.stderr || '').trim() || r.error?.message || `exit ${r.status}`})`, 'warn')
  return null
}

// Escape a string as a TOML basic string: wrap in double quotes, escape
// backslash and double-quote (and control chars codex would choke on).
function tomlString(s) {
  const escaped = String(s)
    .replace(/\\/g, '\\\\')
    .replace(/"/g, '\\"')
    .replace(/\n/g, '\\n')
    .replace(/\r/g, '\\r')
    .replace(/\t/g, '\\t')
  return `"${escaped}"`
}

function tomlKey(s) {
  return tomlString(s)
}

// TOML array of strings: ["a","b"]
function tomlStringArray(arr) {
  return `[${arr.map(tomlString).join(',')}]`
}

// TOML inline table of string values: {KEY="val",KEY2="val2"}
function tomlInlineTable(obj) {
  const parts = Object.entries(obj).map(([k, v]) => `${tomlKey(k)}=${tomlString(v)}`)
  return `{${parts.join(',')}}`
}

function envNamePart(s) {
  const cleaned = String(s).replace(/[^A-Za-z0-9_]/g, '_').replace(/^_+|_+$/g, '')
  return (cleaned || 'SERVER').toUpperCase()
}

function buildMcpConfig() {
  const raw = process.env.PUPPETEER_CODEX_MCP
  const env = { ...process.env }
  // Keep the resolved secret bundle in the Node sidecar only; Codex receives the
  // specific config/env it needs below.
  delete env.PUPPETEER_CODEX_MCP
  delete env.PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS
  delete env.PUPPETEER_CODEX_SEED_FRAMING
  delete env.PUPPETEER_CODEX_STYLE_INSTRUCTIONS
  delete env.PUPPETEER_CODEX_SEED_STYLE
  if (!raw) return { args: [], env }
  let servers
  try {
    servers = JSON.parse(raw)
  } catch {
    return { args: [], env }
  }
  if (!servers || typeof servers !== 'object') return { args: [], env }
  const args = []
  let serverIndex = 0
  for (const [name, cfg] of Object.entries(servers)) {
    const currentServerIndex = serverIndex++
    if (!cfg || typeof cfg !== 'object') continue
    // Shared per-server knobs (only for transports we forward below).
    const isHttp = (cfg.type === 'http' || cfg.type === 'streamable_http') && cfg.url
    let headers = cfg.headers
    if (isHttp && cfg.puppeteer_headers_helper) {
      headers = helperHeaders(name, cfg.puppeteer_headers_helper)
      if (!headers) continue
    }
    const forwarded = (cfg.type === 'stdio' && cfg.command) || isHttp
    if (forwarded) {
      const timeoutSec = Number(cfg.tool_timeout_sec) > 0 ? Number(cfg.tool_timeout_sec) : MCP_TOOL_TIMEOUT_SEC
      if (timeoutSec) args.push('-c', `mcp_servers.${name}.tool_timeout_sec=${timeoutSec}`)
      if (typeof cfg.default_tools_approval_mode === 'string' && cfg.default_tools_approval_mode) {
        args.push('-c', `mcp_servers.${name}.default_tools_approval_mode=${tomlString(cfg.default_tools_approval_mode)}`)
      }
    }
    if (cfg.type === 'stdio' && cfg.command) {
      args.push('-c', `mcp_servers.${name}.command=${tomlString(cfg.command)}`)
      if (Array.isArray(cfg.args) && cfg.args.length) {
        args.push('-c', `mcp_servers.${name}.args=${tomlStringArray(cfg.args)}`)
      }
      if (cfg.env && typeof cfg.env === 'object' && Object.keys(cfg.env).length) {
        args.push('-c', `mcp_servers.${name}.env=${tomlInlineTable(cfg.env)}`)
      }
      continue
    }

    if (isHttp) {
      args.push('-c', `mcp_servers.${name}.url=${tomlString(cfg.url)}`)
      if (headers && typeof headers === 'object' && Object.keys(headers).length) {
        const headerEnvRefs = {}
        let i = 0
        for (const [header, value] of Object.entries(headers)) {
          const envName = `PUPPETEER_MCP_HTTP_${envNamePart(name)}_${currentServerIndex}_${i++}`
          env[envName] = String(value)
          headerEnvRefs[header] = envName
        }
        args.push('-c', `mcp_servers.${name}.env_http_headers=${tomlInlineTable(headerEnvRefs)}`)
      }
    }
  }
  return { args, env }
}

// codex features that hand the model a way to act (exec, apps, plugins,
// browser/computer use, sub-agents…). Unknown keys are ignored by codex, so
// keys removed in a later version are harmless.
const NO_TOOLS_FEATURES = [
  'shell_tool', 'unified_exec', 'apps', 'plugins', 'remote_plugin', 'browser_use',
  'browser_use_external', 'in_app_browser', 'computer_use', 'multi_agent',
  'multi_agent_v2', 'image_generation', 'tool_suggest', 'skill_mcp_dependency_install',
  'code_mode_host', 'in_app_local_automation', 'worktrees', 'view_image', 'guardian_approval',
]

// Names of the MCP servers configured in the user's codex config (they would
// otherwise load into a tool-less session). Fail-closed: throws when the list
// cannot be read, so the session never starts with servers left enabled.
function configuredMcpServerNames() {
  const r = spawnSync(CODEX_BIN, ['mcp', 'list', '--json'], {
    cwd: CWD, encoding: 'utf8', timeout: 10_000, stdio: ['ignore', 'pipe', 'ignore'],
  })
  if (r.error) throw new Error(`codex mcp list failed: ${r.error.message}`)
  if (r.status !== 0) throw new Error(`codex mcp list exited with ${r.status ?? r.signal}`)
  let list
  try {
    list = JSON.parse(r.stdout || '')
  } catch {
    throw new Error('codex mcp list printed no JSON')
  }
  if (!Array.isArray(list)) throw new Error('codex mcp list did not return a list')
  return list.map((s) => s?.name).filter((n) => typeof n === 'string' && n)
}

// `-c` overrides that strip every tool source from a tool-less session.
function noToolsArgs() {
  const args = []
  for (const f of NO_TOOLS_FEATURES) args.push('-c', `features.${f}=false`)
  args.push('-c', 'web_search="disabled"')
  // `-c` key paths split on dots and do not accept quoted segments; codex
  // itself only allows [A-Za-z0-9_-] in server names.
  for (const name of configuredMcpServerNames()) {
    if (/^[A-Za-z0-9_-]+$/.test(name)) args.push('-c', `mcp_servers.${name}.enabled=false`)
  }
  return args
}

// The child env without the broker's MCP bundle / framing inputs.
function noToolsEnv() {
  const env = { ...process.env }
  delete env.PUPPETEER_CODEX_MCP
  delete env.PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS
  delete env.PUPPETEER_CODEX_SEED_FRAMING
  delete env.PUPPETEER_CODEX_STYLE_INSTRUCTIONS
  delete env.PUPPETEER_CODEX_SEED_STYLE
  return env
}

// ── codex app-server JSON-RPC plumbing ────────────────────────────────────
// A tool-less session never gets the broker's MCP servers either.
let mcpConfig
try {
  mcpConfig = NO_TOOLS ? { args: noToolsArgs(), env: noToolsEnv() } : buildMcpConfig()
} catch (e) {
  // Never start a tool-less session we could not strip: report and stop.
  // Written synchronously so the error is not lost on exit.
  writeSync(1, JSON.stringify({
    type: '_puppeteer_error',
    error: `Refusing to start a tool-less codex session: ${e?.message || e}`,
  }) + '\n')
  process.exit(1)
}
// Agents never read the OS secret store where Puppeteer keeps PATs / tokens (and the
// sealed secret notes vault): the macOS Keychain, the Linux Secret Service / KWallet,
// the Windows Credential Manager. Codex applies the execpolicy rules under
// $CODEX_HOME/rules in every approval mode, bypass included (verified with
// codex-cli 0.165; not re-checked on the 0.133 minimum), so keep Puppeteer's
// rule file there — like the Claude sidecar's disallowedTools, every OS gets every rule (WSL reaches `cmdkey.exe`). User-level
// on purpose: a project's `.codex/rules` would land in the user's repo. Best effort:
// a write failure is reported, the session still starts.
// This is a deny list, not a security boundary: a prefix rule only matches a
// command's leading words exactly, so another binary or path, other casing or
// argument order, or a script (PowerShell `CredRead`, `PasswordVault`) goes around it.
const KEYCHAIN_RULES_FILE = 'puppeteer-keychain.rules'
const SECRET_SERVICE_BUSES = ['org.freedesktop.secrets', 'org.kde.kwalletd', 'org.kde.kwalletd5', 'org.kde.kwalletd6']
const DEST_FLAGS = SECRET_SERVICE_BUSES.map((bus) => `--dest=${bus}`)
const DBUS_SEND_OPTS = ['--session', '--print-reply', '--print-reply=literal', '--type=method_call']
const KEYCHAIN_RULE_PATTERNS = [
  ['security', ['find-generic-password', 'find-internet-password', 'dump-keychain', 'export']],
  // The stores' own CLIs: they do nothing else.
  [['secret-tool', 'kwallet-query', 'kwalletcli', 'cmdkey', 'cmdkey.exe', 'vaultcmd', 'vaultcmd.exe', 'Get-StoredCredential']],
  // D-Bus clients calling the Secret Service / KWallet, in their usual argument orders.
  ['gdbus', 'call', ['--session', '-e'], ['--dest', '-d'], SECRET_SERVICE_BUSES],
  ['gdbus', 'call', ['--session', '-e'], DEST_FLAGS],
  ['gdbus', 'call', ['--dest', '-d'], SECRET_SERVICE_BUSES],
  ['gdbus', 'call', DEST_FLAGS],
  ['busctl', '--user', 'call', SECRET_SERVICE_BUSES],
  ['busctl', 'call', '--user', SECRET_SERVICE_BUSES],
  ['busctl', 'call', SECRET_SERVICE_BUSES],
  ['dbus-send', DEST_FLAGS],
  ['dbus-send', DBUS_SEND_OPTS, DEST_FLAGS],
  ['dbus-send', DBUS_SEND_OPTS, DBUS_SEND_OPTS, DEST_FLAGS],
  ['dbus-send', DBUS_SEND_OPTS, DBUS_SEND_OPTS, DBUS_SEND_OPTS, DEST_FLAGS],
  [['qdbus', 'qdbus6', 'qdbus-qt5'], SECRET_SERVICE_BUSES],
]
// JSON string lists are valid Starlark list literals.
const KEYCHAIN_RULES = `# Managed by Puppeteer: agents never read the OS secret store. Rewritten on every Puppeteer Codex session.
${KEYCHAIN_RULE_PATTERNS.map((pattern) => `prefix_rule(
    pattern = ${JSON.stringify(pattern)},
    decision = "forbidden",
    justification = "Puppeteer: agents never read the OS secret store",
)
`).join('')}`

function ensureKeychainRules(env) {
  const file = join(env.CODEX_HOME || join(homedir(), '.codex'), 'rules', KEYCHAIN_RULES_FILE)
  try {
    if (readFileSync(file, 'utf8') === KEYCHAIN_RULES) return
  } catch { /* missing: write it */ }
  try {
    mkdirSync(dirname(file), { recursive: true })
    writeFileSync(file, KEYCHAIN_RULES)
  } catch (e) {
    note(`⚠ Could not install Puppeteer's Keychain rule for Codex (${file}): ${e?.message || e}`, 'warn')
  }
}
ensureKeychainRules(mcpConfig.env || process.env)

const codex = spawn(CODEX_BIN, [...mcpConfig.args, 'app-server'], { cwd: CWD, stdio: ['pipe', 'pipe', 'pipe'], env: mcpConfig.env })
let rpcId = 1
const pending = new Map() // request id → resolve
function request(method, params) {
  const id = rpcId++
  return new Promise((resolve) => {
    pending.set(id, resolve)
    codex.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n')
  })
}

function rpcResult(response, method) {
  if (response?.error) {
    const detail = response.error.message || JSON.stringify(response.error)
    throw new Error(`${method} failed: ${detail}`)
  }
  if (!response || !('result' in response)) {
    throw new Error(`${method} failed: missing JSON-RPC result`)
  }
  return response.result
}

function rpcNotify(method, params) {
  codex.stdin.write(JSON.stringify({ jsonrpc: '2.0', method, params }) + '\n')
}
function rpcRespond(id, result) {
  codex.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, result }) + '\n')
}
function rpcError(id, message) {
  codex.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, error: { code: -32601, message } }) + '\n')
}

// ── permission mode → codex policy/sandbox ────────────────────────────────
function policyFor(mode) {
  // Tool-less: read-only sandbox, and every command outside codex's safe
  // allow-list asks — which emitPermission then declines. Env overrides are
  // ignored so a stray tuning knob can't loosen it.
  if (NO_TOOLS) return { approvalPolicy: 'untrusted', sandbox: 'read-only' }
  let base
  switch (mode) {
    case 'plan':              base = { approvalPolicy: 'never',      sandbox: 'read-only' }; break
    // "Ask via UI" (default) must genuinely gate tool calls so a remote/desktop
    // `Deny` actually blocks execution. `on-request` lets Codex auto-run any
    // command it deems safe (and only asks to escalate), so a denied escalation
    // could still run in-sandbox — the P0 "Deny didn't block" bug. `untrusted`
    // asks for every command outside Codex's safe read-only allow-list, and a
    // `cancel`/`decline` on that prompt reliably prevents the command.
    case 'default':           base = { approvalPolicy: 'untrusted',  sandbox: 'workspace-write' }; break
    // Claude's `acceptEdits` has no one-to-one Codex policy. `on-request` with
    // workspace-write is the closest equivalent: edits inside the workspace are
    // sandboxed and allowed, while actions that need more access can still ask.
    // Codex 0.144+ rejects the old `on-failure` value during thread/start;
    // `on-request` is in the 0.133.0-alpha.1 app-server schema too.
    case 'acceptEdits':       base = { approvalPolicy: 'on-request', sandbox: 'workspace-write' }; break
    case 'bypassPermissions': base = { approvalPolicy: 'never',      sandbox: 'danger-full-access' }; break
    // `auto` (classifier) / `dontAsk` / legacy: let Codex decide when to ask.
    default:                  base = { approvalPolicy: 'on-request', sandbox: 'workspace-write' }
  }
  // Explicit overrides (tuning knob / tests).
  if (process.env.PUPPETEER_CODEX_APPROVAL) base.approvalPolicy = process.env.PUPPETEER_CODEX_APPROVAL
  if (process.env.PUPPETEER_CODEX_SANDBOX) base.sandbox = process.env.PUPPETEER_CODEX_SANDBOX
  return base
}

// ── pending permission requests: codex rpc id ↔ puppeteer requestId ────────────
const permWaiters = new Map() // requestId → { rpcId, command, kind, ... }
// Commands the user denied — so the matching commandExecution item renders as
// rejected (not as a succeeded run).
const declinedCommands = new Set()
const cmdKey = (c) => (Array.isArray(c) ? c.join(' ') : String(c ?? '')).trim()

function emitPermission(rpcReqId, { tool_name, tool_input, title, description, display_name }, waiter = {}) {
  const requestId = `codex-${PROC_NONCE}-${rpcReqId}`
  permWaiters.set(requestId, { rpcId: rpcReqId, command: tool_input?.command, kind: 'approval', ...waiter })
  if (NO_TOOLS) {
    settleWaiter(requestId, false)
    ctrl('_puppeteer_log', { level: 'warn', text: `⚠ ${tool_name} was denied: this session has no tool access.` })
    return
  }
  if (inputClosed) {
    // No answer can reach us any more: decline now instead of raising a prompt
    // nobody could ever answer (the turn then winds down on its own).
    settleWaiter(requestId, false)
    ctrl('_puppeteer_log', { level: 'warn', text: `⚠ ${tool_name} was denied: the session can no longer receive permission answers.` })
    return
  }
  ctrl('_puppeteer_permission_request', { requestId, tool_name, tool_input, title, description, display_name })
}

// Answer a pending codex approval / elicitation (from the user's response, or
// declined by us when the input channel closed).
function settleWaiter(requestId, accept, { answers, remember } = {}) {
  const w = permWaiters.get(requestId)
  if (!w) return false
  permWaiters.delete(requestId)
  if (!accept && w.command != null) declinedCommands.add(cmdKey(w.command))
  if (w.kind === 'mcp_elicitation_form') {
    const content = accept ? elicitationContentFromAnswers(answers, w.answerKeys) : null
    emitMcpToolResult(w.toolUseId, accept)
    rpcRespond(w.rpcId, { action: accept ? 'accept' : 'decline', content, _meta: null })
    return true
  }
  if (w.kind === 'mcp_elicitation') {
    emitMcpToolResult(w.toolUseId, accept)
    rpcRespond(w.rpcId, { action: accept ? 'accept' : 'decline', content: null, _meta: null })
    return true
  }
  // Fail closed on a deny. `cancel` both rejects the pending operation and
  // interrupts the turn, preventing a command from racing ahead while the
  // approval response propagates through the remote controller/host path.
  // A later prompt can resume the same conversation normally.
  //
  // An "always" allow additionally opts into codex's OWN session-scoped
  // approval cache (`acceptForSession`), so matching commands / file changes
  // later in the session never come back as a prompt at all — Puppeteer's
  // per-project list would auto-answer them, but only after a full
  // sidecar → host → UI → sidecar round trip.
  const allowDecision = remember
    ? (w.legacy ? 'approved_for_session' : 'acceptForSession')
    : (w.legacy ? 'approved' : 'accept')
  rpcRespond(w.rpcId, { decision: accept ? allowDecision : 'cancel' })
  return true
}

function enumOptions(schema) {
  if (Array.isArray(schema?.enum)) {
    return schema.enum.map((v) => ({ label: String(v), description: '' }))
  }
  if (Array.isArray(schema?.oneOf)) {
    return schema.oneOf.map((opt) => ({
      label: String(opt?.const ?? opt?.title ?? ''),
      description: String(opt?.description ?? opt?.title ?? ''),
    })).filter((opt) => opt.label)
  }
  return []
}

function elicitationQuestions(params) {
  const schema = params?.requestedSchema
  const props = schema?.properties && typeof schema.properties === 'object' ? schema.properties : {}
  const required = new Set(Array.isArray(schema?.required) ? schema.required : [])
  const questions = []
  const answerKeys = {}

  for (const [key, prop] of Object.entries(props)) {
    if (!prop || typeof prop !== 'object') continue
    const title = String(prop.title || key)
    let question = title
    if (answerKeys[question]) question = `${title} (${key})`
    const options = enumOptions(prop)
    if (prop.type === 'boolean' && !options.length) {
      options.push(
        { label: 'true', description: 'Yes' },
        { label: 'false', description: 'No' },
      )
    }
    questions.push({
      header: `${params?.serverName || 'MCP'}${required.has(key) ? ' · required' : ''}`,
      question,
      multiSelect: prop.type === 'array',
      options,
    })
    answerKeys[question] = { key, type: prop.type }
  }
  return { questions, answerKeys }
}

function coerceElicitationAnswer(value, type) {
  if (type === 'boolean') return String(value).trim().toLowerCase() === 'true'
  if (type === 'number' || type === 'integer') {
    const n = Number(value)
    return Number.isFinite(n) ? n : value
  }
  if (type === 'array') {
    return String(value).split(',').map((v) => v.trim()).filter(Boolean)
  }
  return value
}

function elicitationContentFromAnswers(answers, answerKeys) {
  const content = {}
  for (const [question, value] of Object.entries(answers || {})) {
    const meta = answerKeys?.[question]
    if (!meta) continue
    content[meta.key] = coerceElicitationAnswer(value, meta.type)
  }
  return content
}

function pickString(obj, keys) {
  for (const key of keys) {
    const value = obj?.[key]
    if (typeof value === 'string' && value.trim()) return value.trim()
  }
  return ''
}

function mcpToolUseId(rpcReqId) {
  return `mcp_${String(rpcReqId).replace(/[^A-Za-z0-9_-]/g, '_')}`
}

function mcpDisplayName(params, fallback) {
  return pickString(params, ['displayName', 'toolDisplayName', 'toolTitle', 'title', 'toolName', 'name']) || fallback
}

function mcpToolKeyPart(value, fallback) {
  const clean = String(value ?? '')
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 80)
  return clean || fallback
}

// Stable key for Puppeteer's "allow always" / "deny always" store. Codex's
// `mcpServer/elicitation/request` carries only `serverName` — there is no
// `toolName` in the app-server v2 protocol — so the finest grain we can offer
// is PER SERVER (`mcp__pms`). Returning a bare `MCP` here is what used to make
// the prompt unrememberable, so every MCP request asked again forever.
// `toolName` is still honored when a future codex build starts sending it.
function mcpPermissionToolName(params, server) {
  const serverKey = mcpToolKeyPart(server, '')
  if (!serverKey) return 'MCP'
  const tool = pickString(params, ['toolName', 'name'])
  if (!tool) return `mcp__${serverKey}`
  return `mcp__${serverKey}__${mcpToolKeyPart(tool, 'request')}`
}

function mcpServerDisplayName(params, server) {
  return pickString(params, ['serverDisplayName', 'serverTitle']) || server
}

function mcpTranscriptInput(params, server, questions = []) {
  const schema = params?.requestedSchema
  const fields = schema?.properties && typeof schema.properties === 'object'
    ? Object.keys(schema.properties)
    : []
  return {
    serverName: server,
    mode: params?.mode,
    message: params?.message,
    url: params?.url,
    toolName: pickString(params, ['toolName', 'name']) || undefined,
    ...(fields.length ? { fields } : {}),
    ...(questions.length
      ? {
          questions: questions.map((q) => ({
            header: q.header,
            question: q.question,
            options: q.options.map((o) => o.label),
          })),
        }
      : {}),
  }
}

function emitMcpToolUse(rpcReqId, params, { displayName, input }) {
  const id = mcpToolUseId(rpcReqId)
  const server = input.serverName || params?.serverName || 'MCP server'
  out({
    type: 'assistant',
    message: {
      role: 'assistant',
      content: [{ type: 'tool_use', id, name: 'MCP', input }],
    },
    tool_use_meta: [{
      id,
      display_name: displayName,
      server_display_name: mcpServerDisplayName(params, server),
    }],
  })
  return id
}

function emitMcpToolResult(toolUseId, accepted) {
  if (!toolUseId) return
  out({
    type: 'user',
    message: {
      role: 'user',
      content: [{
        type: 'tool_result',
        tool_use_id: toolUseId,
        content: accepted ? 'Accepted by user.' : 'Denied by user.',
        is_error: !accepted,
      }],
    },
  })
}

function emitMcpElicitation(rpcReqId, params) {
  const server = params?.serverName || 'MCP server'
  if (params?.mode === 'form') {
    const { questions, answerKeys } = elicitationQuestions(params)
    if (questions.length) {
      const toolUseId = emitMcpToolUse(rpcReqId, params, {
        displayName: mcpDisplayName(params, 'MCP input'),
        input: mcpTranscriptInput(params, server, questions),
      })
      emitPermission(rpcReqId, {
        tool_name: 'AskUserQuestion',
        tool_input: { questions, mcp: { serverName: server, message: params.message, mode: params.mode } },
        title: `${server} requests input`,
        description: params.message,
        display_name: 'MCP input',
      }, { kind: 'mcp_elicitation_form', answerKeys, toolUseId })
      return
    }
  }

  const toolUseId = emitMcpToolUse(rpcReqId, params, {
    displayName: mcpDisplayName(params, 'MCP request'),
    input: mcpTranscriptInput(params, server),
  })
  emitPermission(rpcReqId, {
    tool_name: mcpPermissionToolName(params, server),
    tool_input: {
      serverName: server,
      toolName: pickString(params, ['toolName', 'name']) || undefined,
      mode: params?.mode,
      message: params?.message,
      url: params?.url,
    },
    title: `${server} requests permission`,
    description: params?.message || 'Allow this MCP request to continue?',
    display_name: 'MCP request',
  }, { kind: 'mcp_elicitation', toolUseId })
}

// ── translate codex item → Claude-shaped stream-json ──────────────────────
let threadId = FORK ? null : RESUME_THREAD
let modelName = null
// Token usage. The codex app-server reports it on its own
// `thread/tokenUsage/updated` notification (not on `turn/completed`), with two
// breakdowns: `total` is the thread's running sum over every model request,
// `last` is the most recent request ONLY. A turn that runs tools makes several
// requests, so a turn's usage is `total` at its end minus `total` at its start.
// `total` outlives a resume: codex replays the stored one when we attach to a
// thread, tagged with an old turn's id, and that only moves the baseline.
const ZERO_TOKENS = { input: 0, cached: 0, output: 0 }
// Latest `total` seen. Unknown (null) on a resumed or forked thread until codex
// replays it; a new thread starts at zero.
let latestTotal = null
// The turn in flight: `total` when it started, its id once codex named it, the
// `total` of its latest update, and the sum of its `last`s.
let turnBaseline = null
let turnUsageId = null
let turnEndTotal = null
let turnLastSum = null

// A codex TokenUsageBreakdown as { input, cached, output }, or null when it has
// no counts. Field names vary between codex versions (camelCase since the
// app-server v2 protocol, snake_case before), so probe both.
function tokenBreakdown(raw) {
  if (!raw || typeof raw !== 'object') return null
  const u = raw.usage || raw.token_usage || raw.total_token_usage || raw
  let found = false
  const num = (...keys) => {
    for (const k of keys) if (typeof u[k] === 'number') { found = true; return u[k] }
    return 0
  }
  const b = {
    input: num('inputTokens', 'input_tokens'),
    cached: num('cachedInputTokens', 'cached_input_tokens', 'cache_read_input_tokens'),
    output: num('outputTokens', 'output_tokens'),
  }
  return found ? b : null
}
const addTokens = (a, b) => ({ input: a.input + b.input, cached: a.cached + b.cached, output: a.output + b.output })
const subTokens = (a, b) => ({
  input: Math.max(a.input - b.input, 0),
  cached: Math.max(a.cached - b.cached, 0),
  output: Math.max(a.output - b.output, 0),
})

// Map a breakdown onto the Claude `usage` shape that the Rust drain reads.
// codex's `inputTokens` INCLUDES the cached portion; Claude keeps non-cached
// input and cache_read apart, so subtract. `reasoningOutputTokens` is a part of
// `outputTokens` already (codex's own blended total doesn't add it), so it is
// not added again. Returns null when nothing usable.
function mapCodexUsage(b) {
  if (!b) return null
  const usage = {
    input_tokens: Math.max(b.input - b.cached, 0),
    output_tokens: b.output,
    cache_creation_input_tokens: 0,
    cache_read_input_tokens: b.cached,
  }
  if (!usage.input_tokens && !usage.output_tokens && !usage.cache_read_input_tokens) return null
  return usage
}

function noteTokenUsage(tu, updateTurnId) {
  const total = tokenBreakdown(tu?.total)
  const last = tokenBreakdown(tu?.last || (tu?.total ? null : tu))
  // Ours: a turn is in flight and the update names it, or names no turn (older
  // codex). One naming another turn, or coming before codex named ours (the
  // resume replay can land after turn/start went out), is history: it only
  // moves the baseline. Codex names the turn before any request of it ends.
  const ours = turnActive && (updateTurnId == null || (turnUsageId != null && updateTurnId === turnUsageId))
  if (!ours) {
    if (total) {
      latestTotal = total
      if (turnActive) turnBaseline = total
    }
    return
  }
  // A resumed thread codex replayed no total for: before this turn's first
  // request, the total was this update's total minus that request.
  if (!turnEndTotal && !turnLastSum && !turnBaseline && total && last) turnBaseline = subTokens(total, last)
  if (total) latestTotal = turnEndTotal = total
  if (last) turnLastSum = turnLastSum ? addTokens(turnLastSum, last) : last
}

// The finished turn's usage: how much `total` grew over it. A codex that sends
// only `last` (no `total`) gets the sum of the turn's `last`s instead.
function turnUsage() {
  if (turnEndTotal && turnBaseline) return mapCodexUsage(subTokens(turnEndTotal, turnBaseline))
  return mapCodexUsage(turnLastSum)
}

function emitItem(item) {
  if (!item || typeof item !== 'object') return
  switch (item.type) {
    case 'agentMessage': {
      const text = String(item.text ?? '')
      // `id`, like Claude's message id: what a structured quote of this message
      // is anchored by (UX-29). Made here, once, so the live view and the log
      // (which keeps this line verbatim) agree on it.
      if (text.trim()) out({ type: 'assistant', message: { id: `codex-${randomUUID()}`, role: 'assistant', content: [{ type: 'text', text }] } })
      break
    }
    case 'reasoning': {
      // summaries are usually omitted; surface any text we do get.
      const parts = []
      for (const s of item.summary ?? []) if (typeof s === 'string') parts.push(s)
      for (const c of item.content ?? []) if (c?.text) parts.push(c.text)
      const text = parts.join('\n').trim()
      if (text) out({ type: 'assistant', message: { role: 'assistant', content: [{ type: 'thinking', thinking: text }] } })
      break
    }
    case 'commandExecution': {
      out({ type: 'assistant', message: { role: 'assistant', content: [
        { type: 'tool_use', id: item.id, name: 'Bash', input: { command: item.command, description: (item.commandActions?.[0]?.name) || undefined } },
      ] } })
      const rejected =
        item.status === 'declined' ||
        declinedCommands.delete(cmdKey(item.command))
      const isError = rejected || (item.exitCode != null && item.exitCode !== 0)
      const output = rejected
        ? 'Rejected by user — command was not run.'
        : (item.aggregatedOutput != null ? String(item.aggregatedOutput) : '')
      out({ type: 'user', message: { role: 'user', content: [
        { type: 'tool_result', tool_use_id: item.id, content: output, is_error: isError },
      ] } })
      break
    }
    case 'fileChange': {
      // Map to an Edit-like tool_use so the frontend diff view can render it.
      out({ type: 'assistant', message: { role: 'assistant', content: [
        { type: 'tool_use', id: item.id, name: 'Edit', input: item },
      ] } })
      break
    }
    // userMessage: skip — the broker already logs the synthetic user entry.
    default: break
  }
}

// ── handle a message from codex app-server ────────────────────────────────
function onServerMessage(msg) {
  // server → client REQUEST (approval / elicitation)
  if (msg.method && msg.id != null && !('result' in msg) && !('error' in msg)) {
    const p = msg.params || {}
    switch (msg.method) {
      case 'item/commandExecution/requestApproval':
        emitPermission(msg.id, { tool_name: 'Bash', tool_input: { command: p.command }, title: p.reason || 'Run a shell command', description: p.command })
        return
      case 'execCommandApproval':
        emitPermission(msg.id, { tool_name: 'Bash', tool_input: { command: Array.isArray(p.command) ? p.command.join(' ') : p.command }, title: p.reason || 'Run a shell command' }, { legacy: true })
        return
      case 'item/fileChange/requestApproval':
        emitPermission(msg.id, { tool_name: 'Edit', tool_input: p, title: p.reason || 'Apply a file change' })
        return
      case 'applyPatchApproval':
        emitPermission(msg.id, { tool_name: 'Edit', tool_input: p, title: p.reason || 'Apply a patch' }, { legacy: true })
        return
      case 'mcpServer/elicitation/request':
        if (process.env.PUPPETEER_CODEX_MCP_TRACE) {
          note(`[codex] MCP elicitation from ${p.serverName ?? 'server'}: ${p.mode ?? 'unknown'}`, 'debug')
        }
        emitMcpElicitation(msg.id, p)
        return
      case 'item/tool/requestUserInput':
        rpcRespond(msg.id, { answers: {} })
        return
      case 'item/permissions/requestApproval':
        // Grant nothing this turn (≈ decline); per-command approvals still flow
        // through item/commandExecution/requestApproval above.
        rpcRespond(msg.id, { permissions: {}, scope: 'turn' })
        return
      // ---- requests we genuinely can't serve → JSON-RPC error (not malformed result) ----
      case 'item/tool/call':
      case 'account/chatgptAuthTokens/refresh':
      case 'attestation/generate':
      default:
        note(`[codex] unsupported server request: ${msg.method}`, 'warn')
        rpcError(msg.id, `unsupported by puppeteer sidecar: ${msg.method}`)
        return
    }
  }

  // response to one of our RPC requests
  if (msg.id != null && ('result' in msg || 'error' in msg)) {
    const resolve = pending.get(msg.id)
    if (resolve) { pending.delete(msg.id); resolve(msg) }
    return
  }

  // notification (streaming)
  if (msg.method) {
    const p = msg.params || {}
    switch (msg.method) {
      case 'item/completed':
        emitItem(p.item)
        break
      case 'turn/started':
        if (turnActive && p.turn?.id) turnUsageId = p.turn.id
        break
      // Usage arrives on its own notification (since the app-server v2 protocol);
      // the turn's share of it is worked out at turn/completed.
      case 'thread/tokenUsage/updated':
        noteTokenUsage(p.tokenUsage || p, p.turnId)
        break
      // Live plan rate-limit updates (5h + weekly windows). Forwarded so the
      // budget badge / settings panel refresh mid-run, like Claude's /usage.
      case 'account/rateLimits/updated':
        emitCodexRateLimits(p)
        break
      case 'turn/completed': {
        const result = { type: 'result', subtype: 'success', num_turns: 1 }
        // Newer protocol: usage came via thread/tokenUsage/updated. Older
        // protocol: it was carried on the turn/completed params themselves.
        const usage = turnUsage() || mapCodexUsage(tokenBreakdown(p))
        if (usage) result.usage = usage
        turnBaseline = turnUsageId = turnEndTotal = turnLastSum = null
        if (modelName) result.model = modelName
        if (turnStartedAt != null) result.duration_ms = Date.now() - turnStartedAt
        out(result)
        ctrl('_puppeteer_done')
        turnActive = false
        turnCompleted = true
        currentTurn = null
        // A prompt that came in during the turn runs now, as a turn of its own.
        if (waitingPrompts.length) startNextWaiting()
        else if (!KEEP_OPEN || inputClosed) shutdown(0)
        break
      }
      case 'error':
        ctrl('_puppeteer_error', { error: typeof p === 'string' ? p : JSON.stringify(p) })
        break
      default: break // ignore deltas/status/etc for v1
    }
  }
}

// ── codex stdout framing (newline-delimited JSON) ─────────────────────────
let rbuf = ''
codex.stdout.on('data', (chunk) => {
  rbuf += chunk.toString()
  let nl
  while ((nl = rbuf.indexOf('\n')) >= 0) {
    const line = rbuf.slice(0, nl).trim()
    rbuf = rbuf.slice(nl + 1)
    if (!line) continue
    let msg
    try { msg = JSON.parse(line) } catch { continue }
    try { onServerMessage(msg) } catch (e) { ctrl('_puppeteer_error', { error: String(e?.stack || e) }) }
  }
})
let estderrBuf = ''
codex.stderr.on('data', (d) => {
  estderrBuf += d.toString()
  let nl
  while ((nl = estderrBuf.indexOf('\n')) >= 0) {
    classifyStderrLine(estderrBuf.slice(0, nl))
    estderrBuf = estderrBuf.slice(nl + 1)
  }
})
// Report how the session ended. Node gives `code: null` when codex died by a
// signal; that is a failure (exit non-zero). The one exception is OUR kill in
// shutdown() (SIGTERM, or codex exiting on it) after a clean end — the turn
// completed, or no turn was running — which is how a session normally ends.
// Anything else (our kill mid-turn, another signal) reports what happened.
codex.on('exit', (code, signal) => {
  const ourKill = shutdownCode !== null && (signal === 'SIGTERM' || signal == null)
  if (ourKill && (shutdownCode !== 0 || turnCompleted || !turnActive)) {
    ctrl('_puppeteer_closed', { code: shutdownCode, signal: null })
    process.exit(shutdownCode)
  }
  ctrl('_puppeteer_closed', { code, signal: signal ?? null })
  process.exit(code ?? (signal ? 1 : 0))
})

// ── session bring-up ──────────────────────────────────────────────────────
let ready = false
let turnStartedAt = null
// A turn was started and hasn't completed (turn/completed) yet.
let turnActive = false
// The last turn started got its turn/completed (reset by the next turn/start).
let turnCompleted = false
// The broker closed our stdin: no prompt or permission answer can arrive.
let inputClosed = false
// The turn in flight, as a promise of its id (null once refused or completed):
// `turn/steer` must name it, and a steer can arrive before turn/start answers.
let currentTurn = null
const promptQueue = []
// Prompts that arrived while a turn was in flight: never a second turn/start
// over it, they start one by one as each turn completes.
const waitingPrompts = []

async function bringUp() {
  rpcResult(await request('initialize', {
    clientInfo: { name: 'puppeteer-codex', title: 'Puppeteer', version: '0.1.0' },
    capabilities: { experimentalApi: true, requestAttestation: false },
  }), 'initialize')
  rpcNotify('initialized', undefined)

  const { approvalPolicy, sandbox } = policyFor(PERMISSION_MODE)
  if (RESUME_THREAD) {
    // No developerInstructions here: Codex ignores them on resume (framing for a
    // resumed thread arrives via SEED_FRAMING instead).
    const method = FORK ? 'thread/fork' : 'thread/resume'
    const result = rpcResult(await request(method, { threadId: RESUME_THREAD }), method)
    threadId = result?.thread?.id || (FORK ? null : RESUME_THREAD)
    // A fork must never go on in the source thread.
    if (FORK && threadId === RESUME_THREAD) throw new Error('Codex returned the source thread for thread/fork')
    modelName = result?.model || null
    // Rewind: drop the turns after the rewind point before any new turn. Done
    // before system.init, which tells the broker the cut is applied; a failure
    // stops here, so no turn runs on the un-rewound thread.
    // Never more than the thread holds: an over-count must not fail every resume.
    const held = result?.thread?.turns?.length
    const numTurns = FORK_KEEP_TURNS !== null
      ? (Number.isInteger(held) ? Math.max(0, held - FORK_KEEP_TURNS) : 0)
      : Number.isInteger(held) ? Math.min(ROLLBACK_TURNS, held) : ROLLBACK_TURNS
    if (numTurns > 0) {
      rpcResult(await request('thread/rollback', { threadId, numTurns }), 'thread/rollback')
    }
  } else {
    const startParams = { cwd: CWD, approvalPolicy, sandbox }
    if (MODEL) startParams.model = MODEL
    const developerInstructions = [DEVELOPER_INSTRUCTIONS, STYLE_INSTRUCTIONS].filter(Boolean).join('\n\n')
    if (developerInstructions) startParams.developerInstructions = developerInstructions
    const result = rpcResult(await request('thread/start', startParams), 'thread/start')
    threadId = result?.thread?.id || null
    modelName = result?.model || null
    latestTotal = ZERO_TOKENS
    // The new thread carries the framing from now on; the broker persists this
    // so a later resume doesn't seed it again.
    if (threadId && DEVELOPER_INSTRUCTIONS) ctrl('_puppeteer_framing_seeded')
  }

  if (!threadId) throw new Error('Codex initialized without a thread id')

  // system.init lets the broker capture session_id (= threadId) for resume.
  out({ type: 'system', subtype: 'init', session_id: threadId, model: modelName, cwd: CWD, tools: NO_TOOLS ? [] : ['Bash', 'Edit'] })
  ctrl('_puppeteer_ready', { threadId, capabilities: { heartbeat_ms: HEARTBEAT_MS } })

  // Snapshot plan usage right away (cheap, no turn) so the badge is fresh from
  // the start of the run. Fire-and-forget; failures are non-fatal.
  request('account/rateLimits/read', undefined)
    .then((r) => emitCodexRateLimits(r?.result))
    .catch(() => {})

  ready = true
  for (const t of promptQueue.splice(0)) (t.steer ? steerTurn : runPrompt)(t.text, t.images)
}

// Lightweight usage probe: initialize the app-server, read the account's plan
// rate-limits WITHOUT starting a thread or running a turn (no quota spend), emit
// them, then exit. Mirrors the Claude sidecar's `usage_probe`. Triggered by the
// Rust `refresh_codex_plan_usage` command via PUPPETEER_CODEX_USAGE_PROBE=1.
async function usageProbe() {
  try {
    rpcResult(await request('initialize', {
      clientInfo: { name: 'puppeteer-codex', title: 'Puppeteer', version: '0.1.0' },
      capabilities: { experimentalApi: true, requestAttestation: false },
    }), 'initialize')
    rpcNotify('initialized', undefined)
    const result = rpcResult(
      await request('account/rateLimits/read', undefined),
      'account/rateLimits/read',
    )
    emitCodexRateLimits(result)
    ctrl('_puppeteer_done')
  } catch (e) {
    ctrl('_puppeteer_error', { error: String(e?.stack || e) })
  } finally {
    shutdown(0)
  }
}

// `images` is an array of { media_type, data(base64) }. Codex app-server takes
// images as an `image` input item whose `url` is a base64 `data:` URL.
function userInput(text, images) {
  const input = []
  if (text) input.push({ type: 'text', text, text_elements: [] })
  if (Array.isArray(images)) {
    for (const img of images) {
      input.push({ type: 'image', url: `data:${img.media_type};base64,${img.data}` })
    }
  }
  if (!input.length) input.push({ type: 'text', text: '', text_elements: [] })
  return input
}

// Map the thread/start `sandbox` string onto the turn/start `sandboxPolicy`
// object shape (all variants only require `type`).
function sandboxPolicyObject(sandbox) {
  switch (sandbox) {
    case 'read-only':          return { type: 'readOnly' }
    case 'danger-full-access': return { type: 'dangerFullAccess' }
    case 'workspace-write':
    default:                   return { type: 'workspaceWrite' }
  }
}

function runPrompt(text, images) {
  if (turnActive) waitingPrompts.push({ text, images })
  else startTurn(text, images)
}

function startNextWaiting() {
  const next = waitingPrompts.shift()
  startTurn(next.text, next.images)
}

async function startTurn(text, images) {
  if (!threadId) { ctrl('_puppeteer_error', { error: 'no thread' }); return }
  turnActive = true
  turnCompleted = false
  turnStartedAt = Date.now()
  turnBaseline = latestTotal
  turnUsageId = turnEndTotal = turnLastSum = null
  // Prepend the one-time framing seed to the first turn of a resumed thread.
  // Claimed up front so a second turn queued meanwhile can't seed it twice.
  const seed = SEED_FRAMING
  const styleSeed = SEED_STYLE
  SEED_FRAMING = null
  SEED_STYLE = null
  const prefix = [seed, styleSeed].filter(Boolean).join('\n\n')
  if (prefix) text = text ? `${prefix}\n\n${text}` : prefix
  // Re-apply the permission policy on EVERY turn (it overrides "for this turn
  // and subsequent turns"). This is essential for RESUMED threads: `thread/resume`
  // does NOT accept a policy, so a resumed session would otherwise keep whatever
  // policy it was first created with — the P0 where a remote `Deny` failed to
  // block because the resumed run silently stayed on `on-request`.
  const { approvalPolicy, sandbox } = policyFor(PERMISSION_MODE)
  // Surface a rejected turn/start (e.g. bad input shape) instead of hanging: the
  // turn outcome otherwise arrives only via the turn/completed notification.
  const starting = request('turn/start', {
    threadId,
    input: userInput(text, images),
    approvalPolicy,
    sandboxPolicy: sandboxPolicyObject(sandbox),
    ...(EFFORT ? { effort: EFFORT } : {}),
  })
  const thisTurn = starting.then((r) => r?.result?.turn?.id ?? null)
  currentTurn = thisTurn
  const res = await starting
  // The reply names the turn too (for a codex that sends no turn/started),
  // unless the turn is already over by now.
  if (currentTurn === thisTurn && !turnUsageId) turnUsageId = res?.result?.turn?.id ?? null
  if (res?.error) {
    turnActive = false
    ctrl('_puppeteer_error', { error: `turn/start failed: ${res.error.message || JSON.stringify(res.error)}` })
    ctrl('_puppeteer_turn_failed')
    if (seed) SEED_FRAMING = seed // not delivered — retry on the next turn
    if (styleSeed) SEED_STYLE = styleSeed
    if (waitingPrompts.length) startNextWaiting()
    else if (inputClosed) shutdown(1)
  } else if (seed) {
    // Accepted: the framing is now in the thread history. Seed only once.
    ctrl('_puppeteer_framing_seeded')
  }
}

// Steer (D4-06): add input to the turn in flight with `turn/steer`, which
// starts no turn. A turn that is over (or a codex without turn/steer) refuses
// it: say so, never replay it as a new turn behind the user's back. Only a
// steer codex took is reported back: the broker logs it then, not on sending.
async function steerTurn(text, images) {
  // Still starting up with the first prompt waiting: steer the turn it starts.
  if (!ready && promptQueue.length) { promptQueue.push({ text, images, steer: true }); return }
  const turnId = await currentTurn
  const res = turnId
    ? await request('turn/steer', { threadId, input: userInput(text, images), expectedTurnId: turnId })
    : null
  if (!turnId || res?.error) {
    note(`⚠ Steer not applied: ${res?.error?.message || 'no turn is running'}. Send it as a new message instead.`, 'warn')
  } else {
    ctrl('_puppeteer_steered', { text, images })
  }
}

// ── stdin from the broker ─────────────────────────────────────────────────
let sbuf = ''
process.stdin.on('data', (chunk) => {
  sbuf += chunk.toString()
  let nl
  while ((nl = sbuf.indexOf('\n')) >= 0) {
    const line = sbuf.slice(0, nl).trim()
    sbuf = sbuf.slice(nl + 1)
    if (!line) continue
    let cmd
    try { cmd = JSON.parse(line) } catch { continue }
    handleCommand(cmd)
  }
})
// stdin closed by the broker = no more input. That must NOT kill a turn in
// flight (mirrors the Claude sidecar): decline the prompts nobody can answer
// any more, let the turn finish, and shut down once it completed.
process.stdin.on('end', () => {
  inputClosed = true
  for (const requestId of [...permWaiters.keys()]) {
    if (settleWaiter(requestId, false)) {
      ctrl('_puppeteer_permission_cancelled', {
        requestId,
        level: 'warn',
        text: '⚠ An open permission request was denied: the session can no longer receive answers.',
      })
    }
  }
  if (!turnActive && !promptQueue.length) shutdown(0)
})

function handleCommand(cmd) {
  switch (cmd.type) {
    case 'prompt':
      if (!ready) promptQueue.push({ text: cmd.text, images: cmd.images })
      else runPrompt(cmd.text, cmd.images)
      break
    case 'steer':
      steerTurn(cmd.text, cmd.images)
      break
    case 'permission_response':
      settleWaiter(cmd.requestId, cmd.decision === 'allow', { answers: cmd.answers, remember: cmd.remember })
      break
    case 'interrupt':
      if (threadId) rpcNotify('turn/interrupt', { threadId }).catch?.(() => {})
      break
    case 'abort':
      shutdown(0)
      break
    default: break
  }
}

// Set once we are tearing the session down ourselves (the exit code we chose).
// Whether codex dying from our kill() is "expected" also depends on the turn
// state below (see the codex 'exit' handler).
let shutdownCode = null

function shutdown(code) {
  if (shutdownCode === null) shutdownCode = code
  if (estderrBuf.trim()) { classifyStderrLine(estderrBuf); estderrBuf = '' }
  try { codex.kill() } catch {}
  setTimeout(() => process.exit(code), 50)
}

if (process.env.PUPPETEER_CODEX_USAGE_PROBE === '1') {
  usageProbe()
} else {
  bringUp().catch((e) => { ctrl('_puppeteer_error', { error: String(e?.stack || e) }); shutdown(1) })
}
