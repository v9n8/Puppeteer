// Puppeteer ssh/scp shim core (ssh-transparent-connect pivot).
//
// A tiny wrapper placed at the FRONT of the sidecar PATH under the names
// `ssh`/`scp`. Unlike the gh/glab shim it does NOT talk to the broker — the
// credential (host key policy + private key in a per-run ssh-agent) is already
// wired by `prepare_ssh_access`. This shim only points ssh/scp at the per-run
// config by inserting `-F <config>` at the FRONT of argv, then execs the real
// binary.
//
// Hard rules (frozen):
//   - When PUPPETEER_SSH_CONFIG is UNSET, behave EXACTLY like the real ssh/scp:
//     exec it with the original argv, adding nothing (BR-304). This keeps normal
//     ssh working for runs whose project maps no server.
//   - Self-exclude the shim dir from PATH when resolving the real binary so the
//     shim never re-invokes itself (infinite recursion).
//   - Never log anything sensitive; propagate the real binary's exit code.

import { spawn } from 'node:child_process'
import { resolveReal } from './shim.mjs'

const EXIT_REAL_NOT_FOUND = 96 // real ssh/scp not on PATH
const EXIT_EXEC_FAILED = 95 // spawning the real binary failed

/**
 * Compute the argv for the real ssh/scp. When `config` is a non-empty string,
 * prepend `-F <config>` so the per-run config wins; otherwise pass argv through
 * unchanged. Pure function (AC-307) — no I/O — so it is unit-testable.
 * @param {string} config  PUPPETEER_SSH_CONFIG value ('' / undefined → unchanged)
 * @param {string[]} argv  arguments after the tool name
 * @returns {string[]}
 */
export function buildArgs(config, argv) {
  const args = Array.isArray(argv) ? argv : []
  return config ? ['-F', config, ...args] : [...args]
}

/**
 * Entry point for a shim. `tool` is 'ssh' or 'scp'.
 * @param {string} tool
 */
export function run(tool) {
  const config = process.env.PUPPETEER_SSH_CONFIG || ''
  const argv = process.argv.slice(2) // drop [node, shimPath]

  const realBin = resolveReal(tool)
  if (!realBin) {
    process.stderr.write(`puppeteer: real ${tool} not found on PATH\n`)
    process.exit(EXIT_REAL_NOT_FOUND)
  }

  // stdio inherited so interactive/streamed remote output flows through.
  const child = spawn(realBin, buildArgs(config, argv), { stdio: 'inherit' })
  child.on('error', () => {
    process.stderr.write(`puppeteer: failed to exec ${tool}\n`)
    process.exit(EXIT_EXEC_FAILED)
  })
  child.on('exit', (code, signal) => {
    process.exit(signal ? 1 : code ?? 1)
  })
}
