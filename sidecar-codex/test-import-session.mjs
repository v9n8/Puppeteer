// AC-19 experiment: with PUPPETEER_CODEX_IMPORT_SESSION (a Claude → Codex
// handoff), codex imports the Claude session JSONL (externalAgentConfig/import)
// and the run goes on in the thread made from it, resumed with this run's
// policy — before system.init and any turn. Whatever goes wrong with the
// import (no such method, no `…/completed`, no new record in the ledger) ends
// in a new thread instead, never in a failed run. A resume or a side task
// never imports.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync, readFileSync, existsSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-framing.mjs')

function runSidecar(name, env) {
  const work = mkdtempSync(join(tmpdir(), `puppeteer-codex-import-${name}-`))
  const rpcOut = join(work, 'rpc.ndjson')
  const session = join(work, 'claude-session.jsonl')
  writeFileSync(session, '{"type":"user","message":{"role":"user","content":"hi"}}\n')
  const childEnv = { ...process.env }
  for (const k of [
    'PUPPETEER_RESUME_SESSION', 'PUPPETEER_ROLLBACK_TURNS', 'PUPPETEER_FORK_SESSION', 'PUPPETEER_FORK_KEEP_TURNS',
    'PUPPETEER_CODEX_DEVELOPER_INSTRUCTIONS', 'PUPPETEER_CODEX_SEED_FRAMING', 'PUPPETEER_CODEX_MCP', 'PUPPETEER_CODEX_VERSION',
    'PUPPETEER_FIXTURE_CODEX', 'PUPPETEER_FIXTURE_IMPORT', 'PUPPETEER_CODEX_IMPORT_SESSION', 'PUPPETEER_CODEX_NO_TOOLS',
  ]) delete childEnv[k]
  const importEnv = env.noImport ? {} : { PUPPETEER_CODEX_IMPORT_SESSION: session }
  delete env.noImport
  return new Promise((resolve) => {
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: {
        ...childEnv, PUPPETEER_PERMISSION_MODE: 'default', PUPPETEER_CODEX_PATH: fixture, PUPPETEER_FRAMING_RPC_OUT: rpcOut,
        CODEX_HOME: work, PUPPETEER_CODEX_VERSION: '0.133.0-alpha.1', ...importEnv, ...env,
      },
    })
    let init = null
    const logs = []
    let buf = ''
    child.stdout.on('data', (d) => {
      buf += d
      let nl
      while ((nl = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, nl); buf = buf.slice(nl + 1)
        try {
          const m = JSON.parse(line)
          if (m.type === 'system' && m.subtype === 'init' && !init) init = m
          if (m.type === '_puppeteer_log') logs.push(m)
        } catch {}
      }
    })
    child.stdin.write(JSON.stringify({ type: 'prompt', text: 'go on' }) + '\n')
    setTimeout(() => {
      try { child.kill() } catch {}
      const rpc = existsSync(rpcOut)
        ? readFileSync(rpcOut, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l))
        : []
      resolve({ methods: rpc.map((r) => r.method), rpc, init, logs, session })
    }, 1500)
  })
}

const warned = (r) => r.logs.some((l) => l.level === 'warn' && /could not import the Claude session/.test(l.text))

// Imported: the new thread is resumed with this run's policy and goes on.
const a = await runSidecar('ok', {})
const imp = a.rpc.find((r) => r.method === 'externalAgentConfig/import')
const item = imp?.params?.migrationItems?.[0]
const resume = a.rpc.find((r) => r.method === 'thread/resume')
const importedOk = a.methods.join(',') === 'externalAgentConfig/import,thread/resume,turn/start' &&
  item?.itemType === 'SESSIONS' && item.details.sessions[0].path === a.session &&
  resume.params.threadId === 'imported-thread' && resume.params.approvalPolicy === 'untrusted' && resume.params.sandbox === 'workspace-write' &&
  a.rpc.find((r) => r.method === 'turn/start').params.threadId === 'imported-thread' &&
  a.init?.session_id === 'imported-thread' && !warned(a)

// The ledger only has an earlier import of this session: not continued.
const b = await runSidecar('stale', { PUPPETEER_FIXTURE_IMPORT: 'stale' })
const staleOk = b.methods.join(',') === 'externalAgentConfig/import,thread/start,turn/start' && b.init?.session_id === 'fixture-thread' && warned(b)

// No record at all, no such method, or no `…/completed` in time: a new thread.
const c = await runSidecar('none', { PUPPETEER_FIXTURE_IMPORT: 'none' })
const noRecordOk = c.methods.join(',') === 'externalAgentConfig/import,thread/start,turn/start' && warned(c)
const d = await runSidecar('unknown', { PUPPETEER_FIXTURE_IMPORT: 'unknown' })
const unknownOk = d.methods.join(',') === 'externalAgentConfig/import,thread/start,turn/start' && warned(d)
const e = await runSidecar('silent', { PUPPETEER_FIXTURE_IMPORT: 'silent', PUPPETEER_CODEX_RPC_TIMEOUT_SCALE: '0.002' })
const silentOk = e.methods.join(',') === 'externalAgentConfig/import,thread/start,turn/start' && warned(e)

// Without the env, on a resume, or in a side task: no import.
const f = await runSidecar('off', { noImport: true })
const offOk = f.methods.join(',') === 'thread/start,turn/start'
const g = await runSidecar('resume', { PUPPETEER_RESUME_SESSION: 'thr-own' })
const resumeOk = g.methods.join(',') === 'thread/resume,turn/start' && g.rpc[0].params.threadId === 'thr-own'
const h = await runSidecar('no-tools', { PUPPETEER_CODEX_NO_TOOLS: '1' })
const noToolsOk = !h.methods.includes('externalAgentConfig/import') && h.methods[0] === 'thread/start'

const pass = !!(importedOk && staleOk && noRecordOk && unknownOk && silentOk && offOk && resumeOk && noToolsOk)
console.log(JSON.stringify({ importedOk, staleOk, noRecordOk, unknownOk, silentOk, offOk, resumeOk, noToolsOk, methods: [a.methods, b.methods, c.methods, d.methods, e.methods, f.methods, g.methods, h.methods], pass }))
process.exit(pass ? 0 : 1)
