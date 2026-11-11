// Every app-server method the sidecar calls is in the method lists of the codex
// versions under fixtures/ (codex-caps.mjs has where each comes from), and the
// capability table agrees with them: a method marked as there is there, one
// marked as missing is missing, and each version keeps a way to drop turns.
// Static: the sidecar's source is read, no codex runs.
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { codexCaps, rewindMethods } from './codex-caps.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const load = (name) => JSON.parse(readFileSync(join(here, 'fixtures', name), 'utf8'))
// The version codexCaps gets for each list; null: a source checkout, not a release.
const versions = [
  { file: 'codex-0.133.0-alpha.1.methods.json', version: '0.133.0-alpha.1' },
  { file: 'codex-d15bf1d.methods.json', version: null },
].map((v) => ({ ...v, ...load(v.file) }))

// Any quoted string in the sidecar that is a client method of some version is a
// call it makes (methods are also picked at run time, as with thread/fork).
const source = readFileSync(join(here, 'index.mjs'), 'utf8')
const literals = new Set([...source.matchAll(/'([A-Za-z]+(?:\/[A-Za-z_]+)*)'/g)].map((m) => m[1]))
const allRequests = new Set(versions.flatMap((v) => v.clientRequests))
const allNotifications = new Set(versions.flatMap((v) => v.clientNotifications))
const requests = [...literals].filter((m) => allRequests.has(m)).sort()
const notifications = [...literals].filter((m) => allNotifications.has(m)).sort()

const failures = []
// The calls every session makes must have been found at all.
for (const m of ['initialize', 'thread/start', 'thread/resume', 'thread/fork', 'turn/start', 'thread/rollback', 'thread/revert']) {
  if (!requests.includes(m)) failures.push(`index.mjs does not call ${m}`)
}
if (!notifications.includes('initialized')) failures.push('index.mjs does not send initialized')

for (const v of versions) {
  const has = new Set([...v.clientRequests, ...v.clientNotifications])
  const caps = codexCaps(v.version)
  for (const m of [...requests, ...notifications]) {
    // A method the table gates is checked against the table below instead.
    if (m in caps) continue
    if (!has.has(m)) failures.push(`${v.label}: ${m} is not in its app-server`)
  }
  for (const [m, cap] of Object.entries(caps)) {
    if (cap === true && !has.has(m)) failures.push(`${v.label}: codexCaps says ${m} is there, it is not`)
    if (cap === false && has.has(m)) failures.push(`${v.label}: codexCaps says ${m} is missing, it is there`)
  }
  // Whatever the thread's history format, one of the calls tried is there.
  for (const mode of [undefined, 'legacy', 'paginated']) {
    if (!rewindMethods(caps, mode).some((m) => has.has(m))) failures.push(`${v.label}: no call to drop turns (${mode ?? 'no historyMode'})`)
  }
}

const pass = failures.length === 0
console.log(JSON.stringify({ requests, notifications, versions: versions.map((v) => v.label), failures, pass }))
process.exit(pass ? 0 : 1)
