// Pure-function unit tests for the gh/glab/aws/az/terraform shim env handling.
// Run with: node --test sidecar-proxy/shim.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'
import fs from 'node:fs'
import { fileURLToPath } from 'node:url'
import { ask, scrubCloudEnv, pathWithoutDir, pathKey, resolveReal, isWindowsBatch, azureMfaRequired, gitAccountSelector, cloudAccountSelector, stripAwsProfileArgs, remoteHost, glabTargetHost } from './shim.mjs'

const inherited = () => ({
  PATH: '/usr/bin',
  AWS_REGION: 'ap-northeast-1',
  AZURE_CONFIG_DIR: '/sandbox/_empty',
  AZURE_EXTENSION_DIR: '/evil/ext',
  AZURE_CLIENT_SECRET: 'x',
  ARM_METADATA_HOSTNAME: 'evil.example',
  ARM_ENVIRONMENT: 'usgovernment',
  MSI_ENDPOINT: 'http://169.254.169.254',
  IDENTITY_HEADER: 'h',
  TF_CLI_ARGS: '-no-color',
  TF_CLI_ARGS_init: '-migrate-state',
  TF_CLI_CONFIG_FILE: '/evil/.terraformrc',
  TERRAFORM_CONFIG: '/evil/.terraformrc',
  TF_PLUGIN_CACHE_DIR: '/evil/plugins',
  TF_LOG: 'INFO',
})

test('az: drops every AZURE_* / ARM_* / identity var, keeps unrelated env', () => {
  const env = scrubCloudEnv(inherited(), 'az')
  for (const k of Object.keys(env)) {
    assert.ok(!k.startsWith('AZURE_') && !k.startsWith('ARM_'), k)
  }
  assert.equal(env.MSI_ENDPOINT, undefined)
  assert.equal(env.IDENTITY_HEADER, undefined)
  assert.equal(env.PATH, '/usr/bin')
  assert.equal(env.AWS_REGION, 'ap-northeast-1')
  // TF vars are terraform-only concerns.
  assert.equal(env.TF_CLI_ARGS_init, '-migrate-state')
})

test('terraform: also drops TF_CLI_ARGS*, CLI config and plugin cache overrides', () => {
  const env = scrubCloudEnv(inherited(), 'terraform')
  for (const k of ['TF_CLI_ARGS', 'TF_CLI_ARGS_init', 'TF_CLI_CONFIG_FILE', 'TERRAFORM_CONFIG', 'TF_PLUGIN_CACHE_DIR', 'ARM_METADATA_HOSTNAME', 'ARM_ENVIRONMENT', 'AZURE_EXTENSION_DIR']) {
    assert.equal(env[k], undefined, k)
  }
  assert.equal(env.TF_LOG, 'INFO')
})

test('other tools are untouched', () => {
  const env = scrubCloudEnv(inherited(), 'gh')
  assert.equal(env.AZURE_EXTENSION_DIR, '/evil/ext')
})

test('pathWithoutDir removes only the given dir', () => {
  assert.equal(pathWithoutDir('/shim:/usr/bin:/shim/:/bin', '/shim', 'linux'), '/usr/bin:/bin')
  // Windows: `;`, any case, quoted entries.
  assert.equal(
    pathWithoutDir('C:\\App\\shim;"C:\\Program Files\\GitHub CLI";c:\\app\\SHIM\\;C:\\bin', 'C:\\App\\shim', 'win32'),
    'C:\\Program Files\\GitHub CLI;C:\\bin',
  )
})

test('azureMfaRequired spots Entra / ARM MFA rejections only', () => {
  assert.ok(azureMfaRequired('ERROR: AADSTS50076: Due to a configuration change made by your administrator, you must use multi-factor authentication'))
  assert.ok(azureMfaRequired('AADSTS50079: The user is required to use multi-factor authentication.'))
  assert.ok(azureMfaRequired("ERROR: (RequestDisallowedByAzure) Resource 'vm' was disallowed by Azure: ... without authenticating through MFA."))
  assert.ok(!azureMfaRequired('ERROR: (RequestDisallowedByAzure) Resource was disallowed by policy.'))
  assert.ok(!azureMfaRequired('AADSTS700082: The refresh token has expired due to inactivity.'))
  assert.ok(!azureMfaRequired('ERROR: (ResourceNotFound) not found'))
})

test('gitAccountSelector: trimmed PUPPETEER_GIT_ACCOUNT, blank/unset → default', () => {
  assert.equal(gitAccountSelector({ PUPPETEER_GIT_ACCOUNT: ' work ' }), 'work')
  assert.equal(gitAccountSelector({ PUPPETEER_GIT_ACCOUNT: '  ' }), null)
  assert.equal(gitAccountSelector({}), null)
})

test('cloudAccountSelector: trimmed PUPPETEER_AWS_ACCOUNT / PUPPETEER_AZURE_ACCOUNT, blank/unset → default', () => {
  assert.equal(cloudAccountSelector({ PUPPETEER_AWS_ACCOUNT: ' prod ' }, 'PUPPETEER_AWS_ACCOUNT'), 'prod')
  assert.equal(cloudAccountSelector({ PUPPETEER_AWS_ACCOUNT: 'prod' }, 'PUPPETEER_AZURE_ACCOUNT'), null)
  assert.equal(cloudAccountSelector({ PUPPETEER_AZURE_ACCOUNT: ' ' }, 'PUPPETEER_AZURE_ACCOUNT'), null)
})

test('stripAwsProfileArgs: drops --profile and reports its value', () => {
  assert.deepEqual(stripAwsProfileArgs(['s3', 'ls', '--profile', 'prod']), { argv: ['s3', 'ls'], stripped: true, profile: 'prod' })
  assert.deepEqual(stripAwsProfileArgs(['--profile=dev', 'sts', 'get-caller-identity']), { argv: ['sts', 'get-caller-identity'], stripped: true, profile: 'dev' })
  assert.deepEqual(stripAwsProfileArgs(['s3', 'ls']), { argv: ['s3', 'ls'], stripped: false, profile: null })
})

test('remoteHost: https / ssh / scp-like remotes, null otherwise', () => {
  assert.equal(remoteHost('https://oauth2@Git.Corp.io:8443/g/r.git'), 'git.corp.io')
  assert.equal(remoteHost('ssh://git@gitlab.com:2222/g/r'), 'gitlab.com')
  assert.equal(remoteHost('git@git.corp.io:group/r.git'), 'git.corp.io')
  assert.equal(remoteHost('group/repo'), null)
  assert.equal(remoteHost(''), null)
  assert.equal(remoteHost(null), null)
})

test('ask: one NDJSON round trip over the broker endpoint (named pipe on Windows)', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'puppeteer-shim-'))
  const endpoint = process.platform === 'win32'
    ? `\\\\.\\pipe\\${path.basename(dir)}`
    : path.join(dir, 'b.sock')
  let received = null
  const server = net.createServer((c) => {
    let buf = ''
    c.on('data', (d) => {
      buf += d
      const nl = buf.indexOf('\n')
      if (nl < 0) return
      received = JSON.parse(buf.slice(0, nl))
      c.end(JSON.stringify({ decision: 'deny', reason: 'test' }) + '\n')
    })
  })
  await new Promise((r) => server.listen(endpoint, r))
  try {
    const resp = await ask(endpoint, { project_id: 'p1', tool: 'gh', argv: ['pr', 'list'], host: null })
    assert.deepEqual(resp, { decision: 'deny', reason: 'test' })
    assert.deepEqual(received, { project_id: 'p1', tool: 'gh', argv: ['pr', 'list'], host: null })
  } finally {
    await new Promise((r) => server.close(r))
    fs.rmSync(dir, { recursive: true, force: true })
  }
})

test('glabTargetHost: -R URL > GITLAB_HOST > origin remote', () => {
  const origin = () => 'git@git.corp.io:g/r.git'
  assert.equal(glabTargetHost(['mr', 'list', '-R', 'https://gitlab.com/g/r'], { GITLAB_HOST: 'x.io' }, origin), 'gitlab.com')
  assert.equal(glabTargetHost(['mr', 'list', '--repo=git@a.io:g/r'], {}, origin), 'a.io')
  // A plain OWNER/REPO does not name a host.
  assert.equal(glabTargetHost(['mr', 'list', '-R', 'g/r'], { GITLAB_HOST: 'https://x.io' }, origin), 'x.io')
  assert.equal(glabTargetHost(['mr', 'list'], { GL_HOST: 'Y.io' }, origin), 'y.io')
  assert.equal(glabTargetHost(['mr', 'list'], {}, origin), 'git.corp.io')
  assert.equal(glabTargetHost(['mr', 'list'], {}, () => null), null)
})

test('resolveReal skips the shim dir and, on Windows, follows PATHEXT', () => {
  const on = (...files) => (c) => files.includes(c)
  assert.equal(
    resolveReal('gh', { pathValue: '/shim:/usr/bin:/opt/bin', dir: '/shim', platform: 'darwin', runnable: on('/shim/gh', '/opt/bin/gh') }),
    '/opt/bin/gh',
  )
  const win = { pathValue: 'C:\\shim;C:\\Windows;C:\\gh', dir: 'C:\\shim', platform: 'win32', pathext: '.COM;.EXE;.BAT;.CMD' }
  // The shim's own gh.cmd and the extensionless script are never picked.
  assert.equal(resolveReal('gh', { ...win, runnable: on('C:\\shim\\gh.cmd', 'C:\\shim\\gh', 'C:\\gh\\gh.exe') }), 'C:\\gh\\gh.exe')
  assert.equal(resolveReal('gh', { ...win, runnable: on('C:\\gh\\gh') }), null)
  // First match in PATHEXT order within a dir, first dir wins — as cmd does.
  assert.equal(resolveReal('az', { ...win, runnable: on('C:\\Windows\\az.cmd', 'C:\\gh\\az.exe') }), 'C:\\Windows\\az.cmd')
})

test('isWindowsBatch: .cmd / .bat on Windows only', () => {
  assert.equal(isWindowsBatch('C:\\az\\az.CMD', 'win32'), true)
  assert.equal(isWindowsBatch('C:\\x\\t.bat', 'win32'), true)
  assert.equal(isWindowsBatch('C:\\gh\\gh.exe', 'win32'), false)
  assert.equal(isWindowsBatch('/usr/bin/x.cmd', 'linux'), false)
})

test('pathKey finds the inherited spelling on Windows', () => {
  assert.equal(pathKey({ Path: 'C:\\bin' }, 'win32'), 'Path')
  assert.equal(pathKey({}, 'win32'), 'PATH')
  assert.equal(pathKey({ Path: '/x' }, 'linux'), 'PATH')
})

test('every PATH shim has a Windows .cmd wrapper that runs it with node', () => {
  const dir = path.dirname(fileURLToPath(import.meta.url))
  for (const tool of ['gh', 'glab', 'aws', 'az', 'terraform', 'ssh', 'scp']) {
    assert.ok(fs.readFileSync(path.join(dir, tool), 'utf8').startsWith('#!/usr/bin/env node'), tool)
    assert.match(fs.readFileSync(path.join(dir, `${tool}.cmd`), 'utf8'), new RegExp(`^@node "%~dp0${tool}" %\\*\\r?$`, 'm'), tool)
  }
})
