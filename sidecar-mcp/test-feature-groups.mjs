#!/usr/bin/env node
// Feature switches on the built-in `puppeteer` MCP server: the groups listed in
// PUPPETEER_MCP_DISABLED_GROUPS (notes, todos, slack, vps) are missing from
// tools/list and their tools answer tools/call as unknown; every other tool
// stays.
//
// Builds a throwaway DB from every real migration, then drives `index.mjs`
// over stdio. No network.
//
// Usage: node sidecar-mcp/test-feature-groups.mjs

import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { disabledGroups, dropGroups, toolGroup } from './lib/feature-groups.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const SERVER = join(HERE, 'index.mjs');
const MIGRATIONS = join(HERE, '..', 'src-tauri', 'migrations');
const PROJECT = 'proj-1';
const T0 = '2026-01-01T00:00:00Z';

const failures = [];
const check = (ok, label) => {
  if (!ok) failures.push(label);
  process.stdout.write(`${ok ? '  ok  ' : '  FAIL'} ${label}\n`);
};

// ---- Pure helpers ----------------------------------------------------------------

check([...disabledGroups(' todos, ,vps ')].join() === 'todos,vps', 'disabledGroups: trims and drops blanks');
check(disabledGroups(undefined).size === 0 && disabledGroups('').size === 0, 'disabledGroups: unset or empty hides nothing');
check(
  toolGroup('notes_list') === 'notes' && toolGroup('todos_add') === 'todos' &&
    toolGroup('slack_threads_read') === 'slack' && toolGroup('vps_run') === 'vps',
  'toolGroup: prefixes map to groups',
);
check(toolGroup('sessions_search') === null && toolGroup('skills_list') === null, 'toolGroup: core tools have no group');
const reg = dropGroups({ todos_add: 1, notes_list: 2, git_status: 3 }, disabledGroups('todos,bogus'));
check(Object.keys(reg).join() === 'notes_list,git_status', 'dropGroups: removes only the disabled groups');

// ---- Fixture -------------------------------------------------------------------

const appData = mkdtempSync(join(tmpdir(), 'puppeteer-feature-groups-'));
const dbPath = join(appData, 'data.db');
const db = new DatabaseSync(dbPath);
for (const f of readdirSync(MIGRATIONS).filter((f) => /^\d{4}_.*\.sql$/.test(f)).sort()) {
  db.exec(readFileSync(join(MIGRATIONS, f), 'utf8'));
}
db.prepare("INSERT INTO projects (id, name, path, created_at) VALUES (?, 'proj', '/tmp/proj-1', ?)").run(PROJECT, T0);

// ---- Drive the server -------------------------------------------------------------

// Sends `requests` ([method, params]) with ids 1..n; resolves to the replies by id.
function rpc(groups, requests) {
  return new Promise((resolve, reject) => {
    const env = { ...process.env, PUPPETEER_DB_PATH: dbPath, PUPPETEER_PROJECT_ID: PROJECT };
    delete env.PUPPETEER_MCP_DISABLED_GROUPS;
    if (groups != null) env.PUPPETEER_MCP_DISABLED_GROUPS = groups;
    const child = spawn(process.execPath, [SERVER], { env, stdio: ['pipe', 'pipe', 'pipe'] });
    let out = '';
    child.stdout.on('data', (d) => (out += d));
    child.on('error', reject);
    child.on('close', () => {
      const byId = new Map();
      for (const line of out.split('\n').filter((l) => l.trim())) {
        const m = JSON.parse(line); // only JSON-RPC frames may reach stdout
        byId.set(m.id, m);
      }
      resolve(requests.map((_, i) => byId.get(i + 1)));
    });
    requests.forEach(([method, params], i) =>
      child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: i + 1, method, params })}\n`),
    );
    child.stdin.end();
  });
}

const names = (reply) => (reply?.result?.tools ?? []).map((t) => t.name);
const groupsIn = (list) => new Set(list.map(toolGroup).filter(Boolean));

// Unset: every group is there.
let [list] = await rpc(null, [['tools/list', {}]]);
const all = names(list);
check(['notes', 'todos', 'slack', 'vps'].every((g) => groupsIn(all).has(g)), 'unset: all four groups listed');

// Todos off (fresh install default for productivity.todos).
let [listed, add, note] = await rpc('todos', [
  ['tools/list', {}],
  ['tools/call', { name: 'todos_add', arguments: { text: 'hidden' } }],
  ['tools/call', { name: 'notes_create', arguments: { content: 'kept' } }],
]);
const withoutTodos = names(listed);
check(!withoutTodos.some((n) => n.startsWith('todos_')), 'todos off: no todos_* in tools/list');
check(withoutTodos.length === all.filter((n) => !n.startsWith('todos_')).length, 'todos off: every other tool still listed');
check(add?.error?.code === -32602 && add.error.message === 'Unknown tool: todos_add', 'todos off: todos_add is an unknown tool');
check(db.prepare('SELECT COUNT(*) AS n FROM todos').get().n === 0, 'todos off: nothing written');
check(/Created note/.test(note?.result?.content?.[0]?.text ?? ''), 'todos off: notes_create still works');

// Everything a switch can hide is off: only core tools stay.
[listed, add] = await rpc('notes,todos,slack,vps', [
  ['tools/list', {}],
  ['tools/call', { name: 'vps_list', arguments: {} }],
]);
const core = names(listed);
check(groupsIn(core).size === 0, 'all off: no notes / todos / slack / vps tool listed');
check(['sessions_search', 'project_info', 'skills_list', 'rules_list'].every((n) => core.includes(n)), 'all off: core tools stay');
check(add?.error?.message === 'Unknown tool: vps_list', 'all off: vps_list is an unknown tool');

db.close();
rmSync(appData, { recursive: true, force: true });
if (failures.length) {
  process.stdout.write(`\n${failures.length} check(s) failed.\n`);
  process.exit(1);
}
process.stdout.write('\nAll checks passed.\n');
