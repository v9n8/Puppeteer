#!/usr/bin/env node
// `schedule_propose` of the built-in `puppeteer` MCP server (migration 0007,
// src-tauri/src/schedules): an agent can only PROPOSE a schedule — the row is
// off, marked proposed, in plan mode, in the current project — with a timing in
// the JSON shape `schedules::Timing` reads. Bad timings write nothing, and
// pending proposals are capped.
//
// Builds a throwaway DB from every real migration, then drives `index.mjs`
// over stdio. No network.
//
// Usage: node sidecar-mcp/test-schedules.mjs

import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { MAX_PENDING_PROPOSALS, timingOf } from './lib/schedule-tools.mjs';

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
const throws = (fn) => {
  try {
    fn();
    return null;
  } catch (e) {
    return e.message;
  }
};
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

// ---- Pure helpers ----------------------------------------------------------------

check(same(timingOf({ every_minutes: 30 }), { kind: 'interval', every_minutes: 30 }), 'timing: every_minutes → interval');
check(same(timingOf({ at: '08:30' }), { kind: 'daily', time: '08:30' }), 'timing: at → daily');
check(same(timingOf({ at: '08:30', weekdays: [5, 1, 1] }), { kind: 'weekly', weekdays: [1, 5], time: '08:30' }), 'timing: at + weekdays → weekly, sorted, deduped');
check(same(timingOf({ at: '08:30', weekdays: [] }), { kind: 'daily', time: '08:30' }), 'timing: empty weekdays → daily');
check(same(timingOf({ once_at: '2026-11-09T07:15' }), { kind: 'once', at: '2026-11-09T07:15' }), 'timing: once_at → once');
for (const [label, args] of [
  ['none', {}],
  ['two kinds', { every_minutes: 5, at: '08:00' }],
  ['weekdays alone', { weekdays: [1] }],
  ['interval 0', { every_minutes: 0 }],
  ['interval over a week', { every_minutes: 7 * 24 * 60 + 1 }],
  ['interval not whole', { every_minutes: 1.5 }],
  ['time 8:30', { at: '8:30' }],
  ['time 24:00', { at: '24:00' }],
  ['weekday 7', { at: '08:00', weekdays: [7] }],
  ['once with seconds', { once_at: '2026-11-09T07:15:00' }],
  ['once as words', { once_at: 'tomorrow' }],
]) {
  check(throws(() => timingOf(args)) != null, `timing rejected: ${label}`);
}

// ---- Fixture -------------------------------------------------------------------

const appData = mkdtempSync(join(tmpdir(), 'puppeteer-schedules-'));
const dbPath = join(appData, 'data.db');
const db = new DatabaseSync(dbPath);
for (const f of readdirSync(MIGRATIONS).filter((f) => /^\d{4}_.*\.sql$/.test(f)).sort()) {
  db.exec(readFileSync(join(MIGRATIONS, f), 'utf8'));
}
db.prepare("INSERT INTO projects (id, name, path, created_at) VALUES (?, 'proj', '/tmp/proj-1', ?)").run(PROJECT, T0);

// ---- Drive the server -------------------------------------------------------------

function drive(calls, env = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [SERVER], {
      env: { ...process.env, PUPPETEER_DB_PATH: dbPath, PUPPETEER_PROJECT_ID: PROJECT, ...env },
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    let out = '';
    child.stdout.on('data', (d) => (out += d));
    child.on('error', reject);
    child.on('close', () => {
      const byId = new Map();
      for (const line of out.split('\n').filter((l) => l.trim())) {
        const m = JSON.parse(line); // only JSON-RPC frames may reach stdout
        byId.set(m.id, m);
      }
      resolve(Object.fromEntries(calls.map(([label], i) => [label, byId.get(i + 1)])));
    });
    calls.forEach(([, name, args], i) =>
      child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: i + 1, method: 'tools/call', params: { name, arguments: args } })}\n`),
    );
    child.stdin.end();
  });
}
const text = (reply) => reply?.result?.content?.[0]?.text ?? '';
const count = () => db.prepare('SELECT COUNT(*) AS n FROM schedules').get().n;

let out = await drive([
  ['daily', 'schedule_propose', { name: 'Morning PRs', prompt: 'Review the open PRs', at: '08:30', weekdays: [1, 3] }],
  ['bad', 'schedule_propose', { name: 'Broken', prompt: 'x', at: '8:30' }],
  ['no-prompt', 'schedule_propose', { name: 'Empty', prompt: '  ', every_minutes: 30 }],
]);
const rows = db.prepare('SELECT * FROM schedules').all();
check(rows.length === 1, 'propose: one row written, the bad ones none');
const r = rows[0];
check(r.enabled === 0 && r.proposed === 1 && r.next_run_at === null, 'propose: off, proposed, nothing scheduled');
check(r.project_id === PROJECT && r.permission_mode === 'plan' && r.engine === null, 'propose: current project, plan mode, default engine');
check(same(JSON.parse(r.timing), { kind: 'weekly', weekdays: [1, 3], time: '08:30' }), 'propose: timing in the Rust shape');
check(/^Proposed schedule "Morning PRs" \(id [0-9a-f-]{36}\)\. It is OFF/.test(text(out.daily)), 'propose: reply says it is off');
check(out.bad?.result?.isError === true && /`at` must be HH:MM/.test(text(out.bad)), 'propose: bad time is a tool error');
check(out['no-prompt']?.result?.isError === true, 'propose: empty prompt is a tool error');

out = await drive([['no-project', 'schedule_propose', { name: 'n', prompt: 'p', every_minutes: 5 }]], { PUPPETEER_PROJECT_ID: '' });
check(out['no-project']?.result?.isError === true && count() === 1, 'propose: needs a current project');

out = await drive([['hidden', 'schedule_propose', { name: 'n', prompt: 'p', every_minutes: 5 }]], { PUPPETEER_MCP_DISABLED_GROUPS: 'schedules' });
check(out.hidden?.error?.message === 'Unknown tool: schedule_propose' && count() === 1, 'switch off: the tool is unknown, nothing written');

// Pending proposals are capped app-wide; a reviewed one (proposed = 0) does not count.
const fill = Array.from({ length: MAX_PENDING_PROPOSALS }, (_, i) => [`p${i}`, 'schedule_propose', { name: `n${i}`, prompt: 'p', every_minutes: 60 }]);
out = await drive(fill);
const capped = out[`p${MAX_PENDING_PROPOSALS - 1}`];
check(count() === MAX_PENDING_PROPOSALS && capped?.result?.isError === true && /waiting for review/.test(text(capped)), 'cap: the proposal past the cap is refused');
db.prepare('UPDATE schedules SET proposed = 0 WHERE id = ?').run(r.id);
out = await drive([['after-review', 'schedule_propose', { name: 'n', prompt: 'p', every_minutes: 60 }]]);
check(!out['after-review']?.result?.isError && count() === MAX_PENDING_PROPOSALS + 1, 'cap: reviewing one frees a slot');
// A proposal of a project in the trash is hidden in Schedules, so it frees its slot too.
db.prepare("INSERT INTO projects (id, name, path, created_at, deleted_at) VALUES ('gone', 'gone', '/tmp/gone', ?, ?)").run(T0, T0);
db.prepare("UPDATE schedules SET project_id = 'gone' WHERE id = (SELECT id FROM schedules WHERE proposed = 1 LIMIT 1)").run();
out = await drive([['after-trash', 'schedule_propose', { name: 'n', prompt: 'p', every_minutes: 60 }]]);
check(!out['after-trash']?.result?.isError && count() === MAX_PENDING_PROPOSALS + 2, 'cap: a proposal in the trash frees a slot');

db.close();
rmSync(appData, { recursive: true, force: true });
if (failures.length) {
  process.stdout.write(`\n${failures.length} check(s) failed.\n`);
  process.exit(1);
}
process.stdout.write('\nAll checks passed.\n');
