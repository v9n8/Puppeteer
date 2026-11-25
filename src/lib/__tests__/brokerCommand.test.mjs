// Unit tests for lib/brokerCommand.ts (title, risks and host of a credential-broker
// permission prompt: gh, glab, git, aws, az, terraform).
// Run: node --test src/lib/__tests__/brokerCommand.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import {
  brokerAction,
  brokerArgv,
  brokerHost,
  brokerReasonText,
  brokerRisks,
  brokerTitle,
  isBrokerRequest,
} from '../brokerCommand.ts'

const messages = (lang) =>
  JSON.parse(readFileSync(new URL(`../../i18n/locales/${lang}/permission.json`, import.meta.url), 'utf8'))

/** A `t` over the real locale file, with vue-i18n's `{name}` interpolation. */
function translator(lang) {
  const root = { permission: messages(lang) }
  return (key, params = {}) => {
    const msg = key.split('.').reduce((node, k) => node?.[k], root)
    assert.equal(typeof msg, 'string', `${lang}: missing ${key}`)
    return msg.replace(/\{(\w+)\}/g, (_, name) => String(params[name]))
  }
}

/** A broker prompt as the backend sends it: the argv after the tool name. */
const req = (tool, ...argv) => ({
  tool_name: tool,
  tool_input: { command: [tool, ...argv].join(' ') },
  once_only: true,
  broker: { argv },
})
const action = (tool, ...argv) => brokerAction(req(tool, ...argv))
const title = (lang, tool, ...argv) => brokerTitle(req(tool, ...argv), translator(lang))
const risks = (tool, ...argv) => brokerRisks(req(tool, ...argv))

const QUERY = 'query=query{repository(owner:"o",name:"r"){issue(number:1078){title}}}'
const GRAPHQL_READ = ['api', 'graphql', '-f', QUERY, '--jq', '.data', '-c']
const MUTATION = ['api', 'graphql', '-f', 'query=mutation{addComment(input:{}){clientMutationId}}']

test('only a once-only prompt is a broker prompt', () => {
  assert.equal(isBrokerRequest(req('gh', 'pr', 'list')), true)
  assert.equal(isBrokerRequest({ tool_name: 'Bash', tool_input: { command: 'ls' } }), false)
  assert.equal(isBrokerRequest({ tool_name: 'Bash', once_only: false }), false)
})

test('name: a GraphQL query that only reads', () => {
  assert.deepEqual(action('gh', ...GRAPHQL_READ), { key: 'graphqlRead', params: { provider: 'GitHub', tool: 'gh' } })
})

test('name: a GraphQL mutation', () => {
  assert.equal(action('gh', ...MUTATION).key, 'graphqlWrite')
})

test('name: a REST delete names the path', () => {
  assert.deepEqual(action('gh', 'api', '-X', 'DELETE', 'repos/o/r/issues/comments/5'), {
    key: 'apiDelete',
    params: { provider: 'GitHub', path: 'repos/o/r/issues/comments/5' },
  })
})

test('name: a REST call without a method reads', () => {
  assert.equal(action('gh', 'api', 'repos/o/r').key, 'apiRead')
})

test('name: a REST POST writes', () => {
  assert.equal(action('gh', 'api', '-X', 'POST', 'repos/o/r/issues').key, 'apiWrite')
  assert.equal(action('gh', 'api', 'repos/o/r/issues', '-f', 'title=x').key, 'apiWrite')
  assert.equal(action('gh', 'api', '--method=PATCH', 'repos/o/r').key, 'apiWrite')
})

test('name: a PR merge with a number', () => {
  assert.deepEqual(action('gh', 'pr', 'merge', '12'), { key: 'prMergeN', params: { kind: 'PR', n: '12' } })
})

test('name: a PR create', () => {
  assert.deepEqual(action('gh', 'pr', 'create', '--fill'), { key: 'prCreate', params: { kind: 'PR' } })
})

test('name: a PR merge without a number uses the no-number form', () => {
  assert.deepEqual(action('gh', 'pr', 'merge'), { key: 'prMerge', params: { kind: 'PR' } })
})

test('name: an issue edit with a number', () => {
  assert.deepEqual(action('gh', 'issue', 'edit', '34', '--add-label', 'x'), { key: 'issueEditN', params: { n: '34' } })
})

test('name: git push', () => {
  assert.equal(action('git', 'push', 'origin', 'main').key, 'gitPush')
})

test('name: terraform apply and destroy', () => {
  assert.equal(action('terraform', 'apply').key, 'terraformApply')
  assert.equal(action('terraform', 'destroy').key, 'terraformDestroy')
})

test('name: az rest', () => {
  assert.equal(action('az', 'rest', '--url', 'https://management.azure.com/x').key, 'azRest')
})

test('name: aws service and operation', () => {
  assert.deepEqual(action('aws', 's3', 'rm', 's3://b/k'), { key: 'aws', params: { service: 's3', operation: 'rm' } })
})

test('name: aws skips global flags and their values', () => {
  assert.deepEqual(action('aws', '--region', 'eu-west-1', 'ec2', 'describe-instances').params, {
    service: 'ec2',
    operation: 'describe-instances',
  })
})

test('name: a GitLab MR merge', () => {
  assert.deepEqual(action('glab', 'mr', 'merge', '7'), { key: 'prMergeN', params: { kind: 'MR', n: '7' } })
})

test('name: a GitLab GraphQL read with an unknown flag', () => {
  assert.deepEqual(action('glab', 'api', 'graphql', '-f', 'query=query{x}', '-c'), {
    key: 'graphqlRead',
    params: { provider: 'GitLab', tool: 'glab' },
  })
})

test('name: anything else is the tool and its first two words', () => {
  assert.deepEqual(action('gh', 'release', 'upload', 'v1', 'a.zip'), {
    key: 'generic',
    params: { tool: 'gh', words: 'release upload' },
  })
})

test('title: a 200-character path is cut to 80 characters with an ellipsis', () => {
  const path = 'repos/o/r/' + 'x'.repeat(190)
  const text = title('en', 'gh', 'api', '-X', 'POST', path)
  assert.ok(text.length <= 80, `${text.length}`)
  assert.ok(text.endsWith('…'), text)
})

test('title: translated in en and vi', () => {
  assert.equal(title('en', 'gh', ...GRAPHQL_READ), 'Read GitHub data with GraphQL (gh api)')
  assert.equal(title('vi', 'gh', ...GRAPHQL_READ), 'Đọc dữ liệu GitHub bằng GraphQL (gh api)')
  assert.equal(title('vi', 'gh', 'api', '-X', 'DELETE', 'repos/o/r/issues/comments/5'), 'Xoá qua GitHub API · repos/o/r/issues/comments/5')
  assert.equal(title('vi', 'gh', 'pr', 'merge', '12'), 'Merge PR #12')
  assert.equal(title('vi', 'gh', 'pr', 'create', '--fill'), 'Tạo PR')
  assert.equal(title('vi', 'gh', 'issue', 'edit', '34', '--add-label', 'x'), 'Sửa issue #34')
  assert.equal(title('vi', 'git', 'push', 'origin', 'main'), 'Đẩy code lên remote')
  assert.equal(title('vi', 'glab', 'mr', 'merge', '7'), 'Merge MR #7')
  assert.equal(title('vi', 'gh', 'release', 'upload', 'v1', 'a.zip'), 'gh release upload')
  assert.equal(title('vi', 'aws', 's3', 'rm', 's3://b/k'), 'AWS s3: rm')
  assert.equal(title('vi', 'terraform', 'apply'), 'Áp thay đổi hạ tầng (terraform apply)')
  assert.equal(title('vi', 'az', 'rest', '--url', 'x'), 'Gọi Azure REST API')
})

test('risk: reads (GraphQL query, REST GET, list / view) are Read + Network', () => {
  assert.deepEqual(risks('gh', ...GRAPHQL_READ), ['read', 'network'])
  assert.deepEqual(risks('gh', 'api', 'repos/o/r'), ['read', 'network'])
  assert.deepEqual(risks('gh', 'pr', 'view', '12'), ['read', 'network'])
  assert.deepEqual(risks('glab', 'mr', 'list'), ['read', 'network'])
  assert.deepEqual(risks('gh', 'run', 'view', '9'), ['read', 'network'])
})

test('risk: writes (mutation, POST, PR actions, push, apply, az rest POST, unknown) are Write + Network', () => {
  assert.deepEqual(risks('gh', ...MUTATION), ['write', 'network'])
  assert.deepEqual(risks('gh', 'api', '--hostname', 'ghe.corp.example', '-X', 'POST', 'repos/o/r/issues'), ['write', 'network'])
  assert.deepEqual(risks('gh', 'pr', 'merge', '12'), ['write', 'network'])
  assert.deepEqual(risks('git', 'push', 'origin', 'main'), ['write', 'network'])
  assert.deepEqual(risks('terraform', 'apply'), ['write', 'network'])
  assert.deepEqual(risks('az', 'rest', '--method', 'post', '--url', 'x'), ['write', 'network'])
  assert.deepEqual(risks('gh', 'release', 'upload', 'v1', 'a.zip'), ['write', 'network'])
})

test('risk: deletes (REST DELETE, a delete action, destroy, s3 rm, aws delete-) are Delete + Network', () => {
  assert.deepEqual(risks('gh', 'api', '-X', 'DELETE', 'repos/o/r/issues/comments/5'), ['delete', 'network'])
  assert.deepEqual(risks('gh', 'repo', 'delete', 'o/r'), ['delete', 'network'])
  assert.deepEqual(risks('terraform', 'destroy'), ['delete', 'network'])
  assert.deepEqual(risks('aws', 's3', 'rm', 's3://b/k'), ['delete', 'network'])
  assert.deepEqual(risks('aws', 's3', 'rb', 's3://b'), ['delete', 'network'])
  assert.deepEqual(risks('aws', 'ec2', 'terminate-instances', '--instance-ids', 'i-1'), ['delete', 'network'])
  assert.deepEqual(risks('aws', 'sqs', 'delete-queue'), ['delete', 'network'])
})

test('risk: a forced push also deletes', () => {
  assert.deepEqual(risks('git', 'push', '--force'), ['write', 'delete', 'network'])
  assert.deepEqual(risks('git', 'push', '-f', 'origin', 'main'), ['write', 'delete', 'network'])
})

test('risk: cloud reads (aws get / list / describe, az list / show, az rest GET) are Read + Network', () => {
  assert.deepEqual(risks('aws', 'ec2', 'describe-instances', '--frobnicate'), ['read', 'network'])
  assert.deepEqual(risks('aws', 's3api', 'list-buckets'), ['read', 'network'])
  assert.deepEqual(risks('az', 'vm', 'list'), ['read', 'network'])
  assert.deepEqual(risks('az', '-o', 'json', 'account', 'show'), ['read', 'network'])
  assert.deepEqual(risks('az', 'rest', '--method', 'get', '--url', 'x'), ['read', 'network'])
})

test('host: gh defaults to api.github.com, --hostname wins, other tools have none', () => {
  assert.equal(brokerHost(req('gh', ...GRAPHQL_READ)), 'api.github.com')
  assert.equal(brokerHost(req('gh', 'api', '--hostname', 'ghe.corp.example', 'repos/o/r')), 'ghe.corp.example')
  assert.equal(brokerHost(req('gh', 'api', '--hostname=ghe.corp.example', 'repos/o/r')), 'ghe.corp.example')
  assert.equal(brokerHost(req('glab', 'mr', 'list')), null)
  assert.equal(brokerHost(req('aws', 's3', 'ls')), null)
})

test('without a broker part, the argv comes from the shown command', () => {
  const r = { tool_name: 'gh', tool_input: { command: 'gh pr merge 12' }, once_only: true }
  assert.deepEqual(brokerArgv(r), ['pr', 'merge', '12'])
  assert.equal(brokerAction(r).key, 'prMergeN')
})

const has = (lang) => (key) => key.split('.').reduce((node, k) => node?.[k], { permission: messages(lang) }) !== undefined
const ASK = [
  { code: 'apiUnknownFlag', params: { flag: '-c' } },
  { code: 'asAccount', params: { provider: 'GitHub', account: '"me"' } },
]
const asked = (reason) => ({ ...req('gh', ...GRAPHQL_READ), broker: { argv: GRAPHQL_READ, reason } })

test('reason: each part translated and joined', () => {
  assert.equal(
    brokerReasonText(asked(ASK), translator('en'), has('en')),
    'Unknown flag -c: can\'t confirm the call only reads · as GitHub account "me"',
  )
  assert.equal(
    brokerReasonText(asked(ASK), translator('vi'), has('vi')),
    'Cờ lạ -c: không chắc lệnh chỉ đọc · dùng tài khoản GitHub "me"',
  )
})

test('reason: an unknown code or no parts gives null (the English text stays)', () => {
  assert.equal(brokerReasonText(asked([...ASK, { code: 'somethingNew' }]), translator('en'), has('en')), null)
  assert.equal(brokerReasonText(asked([]), translator('en'), has('en')), null)
  assert.equal(brokerReasonText({ tool_name: 'Bash', tool_input: {} }, translator('en'), has('en')), null)
})
