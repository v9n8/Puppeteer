// The permission policy rides on thread/start, thread/resume and thread/fork,
// never on turn/start: a turn/start `sandboxPolicy` of a bare `{ type }` would
// reset network access and writable roots from the user's config.toml on every
// turn. A resumed thread still gets this run's policy (a remote `Deny` must
// block there), and a codex that refuses it on resume, or one below the 0.133
// minimum, falls back to the policy on every turn.
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync, readFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'

const here = dirname(fileURLToPath(import.meta.url))
const fixture = join(here, 'test-fixture-framing.mjs')

function runSidecar(name, env, prompts) {
  const work = mkdtempSync(join(tmpdir(), `puppeteer-codex-policy-${name}-`))
  const rpcOut = join(work, 'rpc.ndjson')
  const childEnv = { ...process.env }
  for (const k of [
    'PUPPETEER_RESUME_SESSION', 'PUPPETEER_FORK_SESSION', 'PUPPETEER_FORK_KEEP_TURNS', 'PUPPETEER_ROLLBACK_TURNS',
    'PUPPETEER_CODEX_APPROVAL', 'PUPPETEER_CODEX_SANDBOX', 'PUPPETEER_CODEX_VERSION', 'PUPPETEER_CODEX_MCP',
    'PUPPETEER_CODEX_NO_TOOLS', 'PUPPETEER_FIXTURE_REFUSE_POLICY', 'PUPPETEER_FIXTURE_TURNS',
  ]) delete childEnv[k]
  return new Promise((resolve) => {
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: work,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: {
        ...childEnv,
        PUPPETEER_PERMISSION_MODE: 'default',
        PUPPETEER_CODEX_PATH: fixture,
        PUPPETEER_FRAMING_RPC_OUT: rpcOut,
        PUPPETEER_FIXTURE_COMPLETE_MS: '100',
        ...env,
      },
    })
    const logs = []
    let buf = ''
    child.stdout.on('data', (d) => {
      buf += d
      let nl
      while ((nl = buf.indexOf('\n')) >= 0) {
        const line = buf.slice(0, nl); buf = buf.slice(nl + 1)
        try {
          const m = JSON.parse(line)
          if (m.type === '_puppeteer_log') logs.push(m)
        } catch {}
      }
    })
    for (const text of prompts) child.stdin.write(JSON.stringify({ type: 'prompt', text }) + '\n')
    setTimeout(() => {
      try { child.kill() } catch {}
      const rpc = existsSync(rpcOut)
        ? readFileSync(rpcOut, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l))
        : []
      const of = (method) => rpc.filter((r) => r.method === method).map((r) => r.params)
      resolve({ of, logs })
    }, 1500)
  })
}

const noPolicy = (p) => !('approvalPolicy' in p) && !('sandboxPolicy' in p)

// New thread: the policy is set once, at thread/start; neither turn re-sends it.
const a = await runSidecar('fresh', {}, ['one', 'two'])
const [start] = a.of('thread/start')
const freshOk = start?.approvalPolicy === 'untrusted' && start?.sandbox === 'workspace-write' &&
  a.of('turn/start').length === 2 && a.of('turn/start').every(noPolicy)

// Resumed thread: the policy goes on thread/resume, and the turns leave the sandbox alone.
const b = await runSidecar('resume', { PUPPETEER_RESUME_SESSION: 'thr-old' }, ['one', 'two'])
const resumes = b.of('thread/resume')
const resumeOk = resumes.length === 1 && resumes[0].threadId === 'thr-old' &&
  resumes[0].approvalPolicy === 'untrusted' && resumes[0].sandbox === 'workspace-write' &&
  b.of('turn/start').length === 2 && b.of('turn/start').every(noPolicy)

// Fork: the copy gets this run's policy (here plan mode's).
const c = await runSidecar('fork', { PUPPETEER_RESUME_SESSION: 'thr-src', PUPPETEER_FORK_SESSION: '1', PUPPETEER_PERMISSION_MODE: 'plan' }, ['one'])
const [fork] = c.of('thread/fork')
const forkOk = fork?.approvalPolicy === 'never' && fork?.sandbox === 'read-only' &&
  c.of('turn/start').length === 1 && c.of('turn/start').every(noPolicy)

// A codex that refuses the policy on resume: resumed without it (logged), and
// every turn carries the policy as before.
const d = await runSidecar('refused', { PUPPETEER_RESUME_SESSION: 'thr-old', PUPPETEER_FIXTURE_REFUSE_POLICY: '1' }, ['one', 'two'])
const dResumes = d.of('thread/resume')
const dTurns = d.of('turn/start')
const refusedOk = dResumes.length === 2 && 'approvalPolicy' in dResumes[0] && !('approvalPolicy' in dResumes[1]) && !('sandbox' in dResumes[1]) &&
  dTurns.length === 2 && dTurns.every((p) => p.approvalPolicy === 'untrusted' && p.sandboxPolicy?.type === 'workspaceWrite') &&
  d.logs.some((l) => l.level === 'warn' && /refused the permission policy/.test(l.text))

// Below the 0.133 minimum the fields may be ignored silently: keep the policy on every turn.
const e = await runSidecar('old', { PUPPETEER_RESUME_SESSION: 'thr-old', PUPPETEER_CODEX_VERSION: '0.130.0' }, ['one'])
const oldOk = e.of('thread/resume')[0]?.approvalPolicy === 'untrusted' &&
  e.of('turn/start').length === 1 && e.of('turn/start')[0].approvalPolicy === 'untrusted' &&
  e.of('turn/start')[0].sandboxPolicy?.type === 'workspaceWrite'

// The 0.133 minimum itself (an alpha build) takes the policy on resume.
const f = await runSidecar('min', { PUPPETEER_RESUME_SESSION: 'thr-old', PUPPETEER_CODEX_VERSION: '0.133.0-alpha.1' }, ['one'])
const minOk = f.of('turn/start').length === 1 && f.of('turn/start').every(noPolicy)

// Auto: the user is not asked, and the workspace sandbox stays the limit.
const g = await runSidecar('auto', { PUPPETEER_PERMISSION_MODE: 'auto' }, ['one'])
const [autoStart] = g.of('thread/start')
const autoOk = autoStart?.approvalPolicy === 'never' && autoStart?.sandbox === 'workspace-write'

const pass = !!(freshOk && resumeOk && forkOk && refusedOk && oldOk && minOk && autoOk)
console.log(JSON.stringify({ freshOk, resumeOk, forkOk, refusedOk, oldOk, minOk, autoOk, pass }))
process.exit(pass ? 0 : 1)
