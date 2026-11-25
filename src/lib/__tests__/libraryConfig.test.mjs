// Unit tests for lib/libraryConfig.ts and the Library's kinds (LH).
// Run: node --test src/lib/__tests__/libraryConfig.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  configRows, configScanErrorRows, isConfigKind, isHooksOffNotice, pluginPartGroups, pluginState, sourceHintKey,
} from '../libraryConfig.ts'
import { kindCounts, libraryKinds, pickTab } from '../libraryTabs.ts'

const hook = (name, command, id = `hook:user:${name}:0:0`) => ({
  kind: 'hook', id, name, description: command, source: 'user', file: '/h/.claude/settings.json', error: null,
  hook: { event: name, matcher: null, hook_type: 'command', command, timeout: null, all_disabled: false }, plugin: null,
})
const plugin = (name, enabled) => ({
  kind: 'plugin', id: `plugin:${name}@m`, name, description: '', source: 'user', file: '/h/.claude/settings.json', error: null,
  hook: null,
  plugin: { key: `${name}@m`, marketplace: 'm', enabled, installed: true, version: null, install_path: null, install_scope: null, parts: null },
})

const items = [hook('Stop', 'say done'), hook('PreToolUse · Bash', 'guard.sh'), plugin('evon', true), plugin('lsp', null)]

test('configRows keeps one kind, sorted by name, ids from the scan', () => {
  assert.deepEqual(configRows(items, 'hook', '').map((r) => r.id), ['hook:user:PreToolUse · Bash:0:0', 'hook:user:Stop:0:0'])
  assert.deepEqual(configRows(items, 'plugin', '').map((r) => r.config.name), ['evon', 'lsp'])
})

test('configRows searches the name, the command and the plugin key', () => {
  assert.deepEqual(configRows(items, 'hook', 'guard').map((r) => r.config.name), ['PreToolUse · Bash'])
  assert.deepEqual(configRows(items, 'plugin', 'lsp@m').map((r) => r.config.name), ['lsp'])
  assert.deepEqual(configRows(items, 'hook', 'evon'), [])
})

test('two hooks of the same name keep their order by id', () => {
  const twins = [hook('Stop', 'b', 'hook:user:Stop:0:1'), hook('Stop', 'a', 'hook:user:Stop:0:0')]
  assert.deepEqual(configRows(twins, 'hook', '').map((r) => r.config.description), ['a', 'b'])
})

test('pluginState reads the most specific value, null as not set', () => {
  assert.equal(pluginState({ enabled: true }), 'on')
  assert.equal(pluginState({ enabled: false }), 'off')
  assert.equal(pluginState({ enabled: null }), 'unset')
})

test('pluginPartGroups keeps the non-empty parts in a fixed order', () => {
  const parts = { skills: ['x'], commands: [], agents: ['g'], hook_events: ['Stop'], mcp_servers: ['srv'], custom_paths: ['agents'] }
  assert.deepEqual(pluginPartGroups(parts).map(({ key }) => key), ['skills', 'agents', 'hook_events', 'mcp_servers'])
})

test('isConfigKind tells hooks and plugins from file kinds', () => {
  assert.equal(isConfigKind('hook'), true)
  assert.equal(isConfigKind('plugin'), true)
  assert.equal(isConfigKind('skill'), false)
  assert.equal(isConfigKind('memory'), false)
})

test('libraryKinds puts hooks and plugins after agents, Memory last on a project', () => {
  assert.deepEqual(libraryKinds('global', false), ['skill', 'rule', 'command', 'agent', 'hook', 'plugin'])
  assert.deepEqual(libraryKinds('project', false), ['skill', 'rule', 'command', 'agent', 'hook', 'plugin', 'memory'])
})

test('libraryKinds leaves hooks and plugins out in remote mode', () => {
  assert.deepEqual(libraryKinds('global', true), ['skill', 'rule', 'command', 'agent'])
  assert.deepEqual(libraryKinds('project', true), ['skill', 'rule', 'command', 'agent', 'memory'])
})

test('config rows count and open their tab like files', () => {
  const rows = items.map((i) => ({ kind: i.kind, path: i.id }))
  const kinds = libraryKinds('global', false)
  assert.deepEqual(kindCounts(rows, kinds), { skill: 0, rule: 0, command: 0, agent: 0, hook: 2, plugin: 2 })
  assert.equal(pickTab(rows, kinds, 'plugin:evon@m', 'skill'), 'plugin')
  assert.equal(pickTab(rows, kinds, null, null), 'hook')
})

test('a failed scan gives one error row per kind, ids apart from file paths', () => {
  const rows = configScanErrorRows('Project not found', 'Claude Code settings', 'project')
  assert.ok(rows.every((r) => r.source === 'project'))
  assert.deepEqual(rows.map((r) => [r.kind, r.id, r.error]), [
    ['hook', 'hook-error:scan', 'Project not found'],
    ['plugin', 'plugin-error:scan', 'Project not found'],
  ])
  assert.deepEqual(configRows(rows, 'plugin', '').map((r) => r.config.name), ['Claude Code settings'])
})

test('sourceHintKey names the install list for a plugin no settings file sets', () => {
  const fromList = { ...plugin('lsp', null), file: '/h/.claude/plugins/installed_plugins.json' }
  assert.equal(sourceHintKey(fromList), 'library.config.sourceHint.installed')
  assert.equal(sourceHintKey({ ...fromList, file: 'C:\\h\\.claude\\plugins\\installed_plugins.json' }), 'library.config.sourceHint.installed')
  assert.equal(sourceHintKey(plugin('evon', true)), 'library.config.sourceHint.user')
  assert.equal(sourceHintKey({ ...hook('Stop', 'x'), source: 'local' }), 'library.config.sourceHint.local')
  const listError = { ...fromList, plugin: null, error: 'not valid JSON' }
  assert.equal(sourceHintKey(listError), 'library.config.sourceHint.installed')
})

test('isHooksOffNotice spots the row of a file that only turns hooks off', () => {
  const notice = { ...hook('disableAllHooks', ''), hook: { ...hook('x', '').hook, event: '', all_disabled: true } }
  assert.equal(isHooksOffNotice(notice), true)
  assert.equal(isHooksOffNotice(hook('Stop', 'x')), false)
  assert.equal(isHooksOffNotice(plugin('evon', true)), false)
})
