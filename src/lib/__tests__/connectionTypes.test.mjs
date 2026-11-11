// Unit tests for lib/connectionTypes.ts (Settings → Connections registry + list).
// Run: node --test src/lib/__tests__/connectionTypes.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  CONNECTION_TYPES, CONNECTION_CONSUMERS, consumerLabelKey, isConnectionTypeId, connectionType,
  enabledConnectionTypes, buildConnectionRows, filterConnectionRows, countConnections, effectiveFilter, checkResult,
} from '../connectionTypes.ts'

const root = new URL('../../../', import.meta.url)
const read = (path) => readFileSync(new URL(path, root), 'utf8')

const prop = (id, effective) => ({
  id, stored: null, effective, locked: false, default: effective, degraded: [], reason_key: null,
})
const treeAll = (on) => [
  { module: 'infra_access', enabled: on, locked: false, props: [prop('infra_access.aws', on), prop('infra_access.azure', on)] },
  { module: 'integrations', enabled: on, locked: false, props: [prop('integrations.google', on)] },
]

// Where each type's account list is loaded and validated from the UI.
const SOURCE_OF = {
  github: 'src/stores/githubAccounts.ts',
  gitlab: 'src/stores/gitlabAccounts.ts',
  aws: 'src/stores/awsAccounts.ts',
  azure: 'src/stores/azureAccounts.ts',
  google: 'src/components/settings/GoogleAccountsSettings.vue',
}

// Proof that each declared consumer really reads that type's accounts: [file, text it contains].
const EVIDENCE = {
  projects: {
    github: [['src/components/projects/ProjectConnections.vue', 'useGithubAccountsStore'], ['src/stores/projects.ts', 'github_account_ids: string[]']],
    gitlab: [['src/components/projects/ProjectConnections.vue', 'useGitlabAccountsStore'], ['src/stores/projects.ts', 'gitlab_account_ids: string[]']],
    aws: [['src/components/projects/ProjectConnections.vue', 'useAwsAccountsStore'], ['src/stores/projects.ts', 'aws_account_ids: string[]']],
    azure: [['src/components/projects/ProjectConnections.vue', 'useAzureAccountsStore'], ['src/stores/projects.ts', 'azure_account_ids: string[]']],
  },
  sessionChips: {
    github: [['src/components/sessions/useRunHeaderInfo.ts', 'ghStore.accounts']],
    gitlab: [['src/components/sessions/useRunHeaderInfo.ts', 'glStore.accounts']],
    aws: [['src/components/sessions/useRunHeaderInfo.ts', 'awsStore.accounts']],
    azure: [['src/components/sessions/useRunHeaderInfo.ts', 'azureStore.accounts']],
  },
  cliBroker: {
    github: [['src-tauri/src/runs/broker/token.rs', 'FROM github_accounts']],
    gitlab: [['src-tauri/src/runs/broker/token.rs', 'FROM gitlab_accounts']],
    aws: [['src-tauri/src/runs/broker/token.rs', 'FROM aws_accounts']],
    azure: [['src-tauri/src/runs/broker/token.rs', 'FROM azure_accounts']],
  },
  prInbox: {
    github: [['src-tauri/src/commands/github.rs', 'FROM github_accounts WHERE username IS NOT NULL'], ['src/views/PrInboxView.vue', 'useGithubAccountsStore']],
  },
  issueLinks: {
    github: [['src-tauri/src/commands/github.rs', 'SELECT 1 FROM github_accounts WHERE id']],
    gitlab: [['src-tauri/src/commands/github.rs', 'FROM gitlab_accounts WHERE id']],
  },
  mcpGitAccounts: {
    github: [['sidecar-mcp/index.mjs', 'git_accounts_list'], ['sidecar-mcp/lib/store.mjs', "'github_accounts'"]],
    gitlab: [['sidecar-mcp/index.mjs', 'git_accounts_list'], ['sidecar-mcp/lib/store.mjs', "'gitlab_accounts'"]],
  },
  mcpCloudAccounts: {
    aws: [['sidecar-mcp/index.mjs', 'cloud_accounts_list'], ['sidecar-mcp/lib/store.mjs', "'aws_accounts'"]],
    azure: [['sidecar-mcp/index.mjs', 'cloud_accounts_list'], ['sidecar-mcp/lib/store.mjs', "'azure_accounts'"]],
  },
  googleMcp: {
    google: [['src-tauri/src/runs/broker/mcp_creds.rs', 'FROM google_accounts']],
  },
  calendar: {
    google: [['src-tauri/src/commands/gcalendar.rs', 'FROM google_accounts']],
  },
}

test('registry: unique ids, every type has a validate command and at least one consumer', () => {
  const ids = CONNECTION_TYPES.map((t) => t.id)
  assert.deepEqual(ids, ['github', 'gitlab', 'aws', 'azure', 'google'])
  assert.equal(new Set(ids).size, ids.length)
  for (const t of CONNECTION_TYPES) {
    assert.match(t.validateCommand, /^validate_\w+_account$/, t.id)
    assert.ok(t.consumers.length >= 1, `${t.id} has no consumer`)
    assert.equal(new Set(t.consumers).size, t.consumers.length, `${t.id} lists a consumer twice`)
    for (const c of t.consumers) assert.ok(CONNECTION_CONSUMERS.includes(c), `${t.id} → ${c}`)
  }
})

test('registry: validate commands are registered Tauri commands the type already calls', () => {
  const lib = read('src-tauri/src/lib.rs')
  const handler = lib.slice(lib.indexOf('generate_handler!['))
  for (const t of CONNECTION_TYPES) {
    assert.match(handler, new RegExp(`\\b${t.validateCommand},`), `${t.validateCommand} not in generate_handler`)
    assert.ok(read(SOURCE_OF[t.id]).includes(`'${t.validateCommand}'`), `${SOURCE_OF[t.id]} does not invoke ${t.validateCommand}`)
  }
})

test('registry: the sign-in marker is the one the type already checks for', () => {
  for (const t of CONNECTION_TYPES) {
    if (!t.loginRequiredMarker) continue
    assert.ok(read(SOURCE_OF[t.id]).includes(`'${t.loginRequiredMarker}'`), `${t.id}: ${t.loginRequiredMarker}`)
  }
})

test('registry: every declared consumer is backed by code that reads the accounts', () => {
  for (const t of CONNECTION_TYPES) {
    for (const c of t.consumers) {
      const proof = EVIDENCE[c]?.[t.id]
      assert.ok(proof?.length, `no evidence for ${t.id} → ${c}`)
      for (const [file, text] of proof) assert.ok(read(file).includes(text), `${file} lacks ${text}`)
    }
  }
  // And no evidence is left for a consumer a type no longer declares.
  for (const [c, byType] of Object.entries(EVIDENCE)) {
    for (const id of Object.keys(byType)) assert.ok(connectionType(id).consumers.includes(c), `${id} → ${c} not declared`)
  }
})

test('registry: project fields exist on Project', () => {
  const projects = read('src/stores/projects.ts')
  for (const t of CONNECTION_TYPES) if (t.projectField) assert.ok(projects.includes(`${t.projectField}: string[]`), t.projectField)
})

test('labels: type, consumer, secret, status and check keys exist in en and vi', () => {
  for (const locale of ['en', 'vi']) {
    const msgs = JSON.parse(read(`src/i18n/locales/${locale}/settings.json`))
    const get = (key) => key.split('.').slice(1).reduce((o, k) => o?.[k], msgs)
    const keys = [
      ...CONNECTION_TYPES.flatMap((t) => [t.labelKey, ...t.secretKeys]),
      ...CONNECTION_CONSUMERS.map(consumerLabelKey),
      ...['validated', 'validatedAt', 'unvalidated', 'needsLogin', 'connected'].map((s) => `settings.connections.status.${s}`),
      'settings.connections.check.ok', 'settings.connections.check.missingScopes', 'settings.github.missingRepoScope',
    ]
    for (const key of keys) assert.equal(typeof get(key), 'string', `${locale} ${key}`)
  }
})

test('isConnectionTypeId: the old Settings section ids, nothing else', () => {
  for (const id of ['github', 'gitlab', 'aws', 'azure', 'google']) assert.ok(isConnectionTypeId(id), id)
  for (const id of ['claude', 'connections', 'inbox', '', null, undefined, ['github']]) assert.ok(!isConnectionTypeId(id), String(id))
})

test('enabledConnectionTypes: AWS, Azure, Google follow their features; GitHub, GitLab always on', () => {
  assert.deepEqual(enabledConnectionTypes(treeAll(true)).map((t) => t.id), ['github', 'gitlab', 'aws', 'azure', 'google'])
  assert.deepEqual(enabledConnectionTypes(treeAll(false)).map((t) => t.id), ['github', 'gitlab'])
  const azureOff = [
    { module: 'infra_access', enabled: true, locked: false, props: [prop('infra_access.aws', true), prop('infra_access.azure', false)] },
    { module: 'integrations', enabled: true, locked: false, props: [prop('integrations.google', true)] },
  ]
  assert.deepEqual(enabledConnectionTypes(azureOff).map((t) => t.id), ['github', 'gitlab', 'aws', 'google'])
})

const sources = {
  // Store order on purpose: rows keep it within a type.
  gitlab: [{ id: 'gl1', label: 'corp', username: 'me', host: 'https://git.corp' }],
  github: [
    { id: 'gh1', label: 'work', username: 'octo' },
    { id: 'gh2', label: 'new', username: null },
  ],
  aws: [{ id: 'a1', label: 'prod', profile_name: 'prod-admin', region: 'ap-northeast-1', account_id: '123', last_validated_at: '2026-11-01T00:00:00Z' }],
  azure: [
    { id: 'z1', label: 'stg', subscription_name: 'Sub', subscription_id: 's-1', user_name: 'me@x', logged_in: false, last_validated_at: '2026-11-01T00:00:00Z' },
    { id: 'z2', label: 'dev', subscription_name: null, subscription_id: 's-2', user_name: null, logged_in: true, last_validated_at: null },
  ],
  google: [{ id: 'g1', label: 'me', email: 'me@gmail.com', is_default: true }],
}
const projects = [
  { github_account_ids: ['gh1'], gitlab_account_ids: [], aws_account_ids: ['a1'], azure_account_ids: [] },
  { github_account_ids: ['gh1', 'gh2'], gitlab_account_ids: [], aws_account_ids: [], azure_account_ids: ['z1'] },
]

test('buildConnectionRows: one row per account, by type then store order, with detail, status and project count', () => {
  const rows = buildConnectionRows(enabledConnectionTypes(treeAll(true)), sources, projects)
  assert.deepEqual(rows.map((r) => `${r.type}:${r.id}`), ['github:gh1', 'github:gh2', 'gitlab:gl1', 'aws:a1', 'azure:z1', 'azure:z2', 'google:g1'])
  const by = Object.fromEntries(rows.map((r) => [r.id, r]))
  assert.deepEqual(by.gh1, {
    type: 'github', id: 'gh1', label: 'work', detail: '@octo', status: 'validated', validatedAt: null, isDefault: false, projectCount: 2,
  })
  assert.equal(by.gh2.status, 'unvalidated')
  assert.equal(by.gh2.detail, '')
  assert.equal(by.gh2.projectCount, 1)
  assert.equal(by.gl1.detail, '@me · https://git.corp')
  assert.equal(by.gl1.projectCount, 0)
  assert.equal(by.a1.detail, 'prod-admin · ap-northeast-1 · 123')
  assert.equal(by.a1.status, 'validated')
  assert.equal(by.a1.validatedAt, '2026-11-01T00:00:00Z')
  assert.equal(by.z1.status, 'needsLogin')
  assert.equal(by.z1.detail, 'Sub · me@x')
  assert.equal(by.z2.status, 'unvalidated')
  assert.equal(by.z2.detail, 's-2')
  // Google accounts are not bound to projects: no count rather than a misleading 0.
  assert.equal(by.g1.projectCount, null)
  assert.equal(by.g1.status, 'connected')
  assert.equal(by.g1.isDefault, true)
})

test('buildConnectionRows: types switched off are left out, a missing source is empty', () => {
  const rows = buildConnectionRows(enabledConnectionTypes(treeAll(false)), sources, projects)
  assert.deepEqual([...new Set(rows.map((r) => r.type))], ['github', 'gitlab'])
  assert.deepEqual(buildConnectionRows(enabledConnectionTypes(treeAll(true)), { github: sources.github }, []).map((r) => r.id), ['gh1', 'gh2'])
})

test('filterConnectionRows and countConnections', () => {
  const on = enabledConnectionTypes(treeAll(true))
  const rows = buildConnectionRows(on, sources, projects)
  assert.equal(filterConnectionRows(rows, 'all').length, 7)
  assert.deepEqual(filterConnectionRows(rows, 'azure').map((r) => r.id), ['z1', 'z2'])
  assert.deepEqual(filterConnectionRows(rows, 'gitlab').map((r) => r.id), ['gl1'])
  assert.equal(countConnections(on, sources), 7)
  // The badge counts only enabled types.
  assert.equal(countConnections(enabledConnectionTypes(treeAll(false)), sources), 3)
})

test('effectiveFilter: a filter on a switched-off type falls back to all', () => {
  const off = enabledConnectionTypes(treeAll(false))
  assert.equal(effectiveFilter('aws', off), 'all')
  assert.equal(effectiveFilter('github', off), 'github')
  assert.equal(effectiveFilter('all', off), 'all')
  assert.equal(effectiveFilter('aws', enabledConnectionTypes(treeAll(true))), 'aws')
})

test('checkResult: sign-in markers, plain errors and warnings in a passing result', () => {
  const gh = connectionType('github')
  const aws = connectionType('aws')
  const google = connectionType('google')
  assert.deepEqual(checkResult(gh, { ok: { username: 'octo', scopes: ['repo'], has_repo_scope: true } }), { outcome: 'ok' })
  assert.deepEqual(checkResult(gh, { ok: { username: 'octo', scopes: [], has_repo_scope: false } }),
    { outcome: 'warn', messageKey: 'settings.github.missingRepoScope' })
  assert.deepEqual(checkResult(gh, { error: 'HTTP 401' }), { outcome: 'error', message: 'HTTP 401' })
  assert.deepEqual(checkResult(aws, { error: 'SSO_LOGIN_REQUIRED: token expired' }),
    { outcome: 'needsLogin', messageKey: 'settings.connections.status.needsLogin' })
  assert.deepEqual(checkResult(connectionType('azure'), { error: new Error('AZURE_LOGIN_REQUIRED') }).outcome, 'needsLogin')
  // GitHub has no sign-in flow: the same text is a plain error there.
  assert.equal(checkResult(gh, { error: 'SSO_LOGIN_REQUIRED' }).outcome, 'error')
  assert.deepEqual(checkResult(google, { ok: { email: 'a', scope: 's', missing_scopes: ['calendar'] } }),
    { outcome: 'needsLogin', messageKey: 'settings.connections.check.missingScopes' })
  assert.deepEqual(checkResult(google, { ok: { email: 'a', scope: 's', missing_scopes: [] } }), { outcome: 'ok' })
})
