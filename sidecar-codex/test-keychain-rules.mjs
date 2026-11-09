// Every Codex session installs Puppeteer's execpolicy rules that forbid reading the
// OS secret store — macOS `security`, Linux `secret-tool` / D-Bus clients, Windows
// `cmdkey` / `vaultcmd` ($CODEX_HOME/rules/puppeteer-keychain.rules) — and puts them
// back when they were changed. Uses the fixture app-server (no CLI).
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import assert from 'node:assert/strict'

const here = dirname(fileURLToPath(import.meta.url))
const codexHome = mkdtempSync(join(tmpdir(), 'puppeteer-codex-home-'))
const rules = join(codexHome, 'rules', 'puppeteer-keychain.rules')

function runSession() {
  const env = { ...process.env, CODEX_HOME: codexHome, PUPPETEER_CODEX_PATH: join(here, 'test-fixture-app-server.mjs') }
  delete env.PUPPETEER_RESUME_SESSION
  return new Promise((resolve) => {
    const child = spawn('node', [join(here, 'index.mjs')], {
      cwd: mkdtempSync(join(tmpdir(), 'puppeteer-codex-rules-')),
      stdio: ['pipe', 'pipe', 'pipe'],
      env,
    })
    const timer = setTimeout(() => child.kill('SIGKILL'), 10_000)
    let buf = ''
    child.stdout.on('data', (chunk) => {
      buf += chunk.toString()
      // Ready means the rule was handled (it is written before codex starts).
      if (buf.includes('"_puppeteer_ready"')) child.stdin.end()
    })
    child.on('exit', () => { clearTimeout(timer); resolve() })
  })
}

await runSession()
const first = readFileSync(rules, 'utf8')
assert.match(first, /prefix_rule\(/)
assert.match(first, /"find-generic-password"/)
assert.match(first, /decision = "forbidden"/)
for (const word of ['secret-tool', 'kwallet-query', 'cmdkey.exe', 'vaultcmd', 'Get-StoredCredential',
  'gdbus', 'busctl', 'dbus-send', 'qdbus', 'org.freedesktop.secrets', '--dest=org.kde.kwalletd5']) {
  assert.ok(first.includes(`"${word}"`), word)
}

writeFileSync(rules, '# tampered\n')
await runSession()
assert.equal(readFileSync(rules, 'utf8'), first)

console.log('ok   codex sessions install the secret store execpolicy rules')
